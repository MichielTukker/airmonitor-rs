#![no_std]
#![no_main]

use core::net::{IpAddr, SocketAddr};
use defmt::{debug, error, flush, info, warn};
use embassy_executor::Spawner;
use embassy_net::{
    DhcpConfig, Runner, Stack, StackResources,
    dns::DnsQueryType,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_time::{Duration, Timer};
use esp_hal::delay::Delay;
use esp_hal::{
    clock::CpuClock,
    gpio::{DriveMode, Flex, OutputConfig, Pull},
    rng::Rng,
    rtc_cntl::Rtc,
    timer::timg::TimerGroup,
};

use esp_println as _; // required to setup defmt global logger
use esp_radio::wifi::{
    ClientConfig, ModeConfig, ScanConfig, WifiController, WifiDevice, WifiEvent, WifiStaState,
};
extern crate alloc;
use alloc::format;
use sntpc::{NtpContext, NtpTimestampGenerator, get_time};

use airmonitor_rs::devices::display;
use embedded_dht_rs::dht22::Dht22;
// use airmonitor_rs::devices::environment::{EnvironmentSensor};

// use airmonitor_rs::time_ntp::ntp::{NTP_SERVER};
// use airmonitor_rs::time_ntp::timestamp::Timestamp;

// use esp_hal::rtc_cntl::Rtc;
// use sntpc::NtpTimestampGenerator;

/// Microseconds in a second
const USEC_IN_SEC: u64 = 1_000_000;

// const TIMEZONE: jiff::tz::TimeZone = jiff::tz::get!("UTC");
const NTP_SERVER: &str = "pool.ntp.org";

#[derive(Clone, Copy)]
struct Timestamp<'a> {
    rtc: &'a Rtc<'a>,
    current_time_us: u64,
}

impl NtpTimestampGenerator for Timestamp<'_> {
    fn init(&mut self) {
        self.current_time_us = self.rtc.current_time_us();
    }

    fn timestamp_sec(&self) -> u64 {
        self.current_time_us / USEC_IN_SEC
    }

    fn timestamp_subsec_micros(&self) -> u32 {
        (self.current_time_us % USEC_IN_SEC) as u32
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

//TODO : Replace with static_cell::make_static when on stable
// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}
const SSID: &str = env!("SSID", "SSID not set");
const PASSWORD: &str = env!("PASSWORD", "PASSWORD not set");

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let rtc = Rtc::new(peripherals.LPWR);

    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 98767);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    // let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    //let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    esp_rtos::start(timg0.timer0);
    info!("Embassy initialized!");

    // Setup display.  SSD1306 OLED via I2C at address 0x3C.
    // SDA = GPIO5, SCL = GPIO4 for our esp32 board
    let mut display =
        display::OledDisplay::new(peripherals.I2C0, peripherals.GPIO5, peripherals.GPIO4);
    display.clear();
    display.print("=== Airmonitor-rs ===");

    // The esp radio controller must be 'static because it is used by the network stack,
    //  which partially runs on the second core.
    let radio_init = &*mk_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
    );
    let (controller, interfaces) =
        esp_radio::wifi::new(radio_init, peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    let wifi_interface = interfaces.sta;
    let rng = Rng::new();
    let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);
    let tls_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

    let dhcp_config = DhcpConfig::default();
    let config = embassy_net::Config::dhcpv4(dhcp_config);

    // Init network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        net_seed,
    );

    spawner.spawn(connection(controller)).ok();
    spawner.spawn(net_task(runner)).ok();

    let address = wait_for_connection(stack).await;

    display.print_at(0, 15, "v0.0.1.");
    display.print_at(0, 28, &format!("IP: {}", address));

    //TODO current time/date?
    //TODO sensor readings (DHT/BME280/PM2.5)
    //TODO implement screen refreshing (clearing pixels before writing new text)
    //TODO add MQTT messaging
    // access_website(stack, tls_seed).await;
    let now = jiff::Timestamp::from_microsecond(rtc.current_time_us() as i64).unwrap();
    // let formatted_ts = now.to_string();
    info!("Rtc: {}", defmt::Display2Format(&now));

    fetch_time_from_ntp(stack, tls_seed, &rtc).await;

    let now = jiff::Timestamp::from_microsecond(rtc.current_time_us() as i64).unwrap();
    // let formatted_ts = now.to_string();
    info!("Rtc: {}", defmt::Display2Format(&now));

    // DHT22 sensor setup (GPIO5)
    let mut dht22_pin = Flex::new(peripherals.GPIO14);
    dht22_pin.apply_output_config(
        &OutputConfig::default()
            .with_drive_mode(DriveMode::OpenDrain)
            .with_pull(Pull::None),
    );
    dht22_pin.set_output_enable(true);
    dht22_pin.set_input_enable(true);
    dht22_pin.set_high();

    let mut dht22 = Dht22::new(dht22_pin, Delay::new());

    // let mut env_sensor = EnvironmentSensor::new(pperipherals.GPIO14.into_open_drain_output());

    loop {
        match dht22.read() {
            Ok(sensor_reading) => {
                info!(
                    "DHT 22 Sensor - Temperature: {} °C, humidity: {} %",
                    sensor_reading.temperature, sensor_reading.humidity
                );
                display.print_at(0, 42, &format!("Temp: {:.1} C", sensor_reading.temperature));
                display.print_at(0, 54, &format!("Hum: {:.1} %", sensor_reading.humidity));
            }
            Err(error) => {
                warn!(
                    "An error occurred while trying to read sensor: {:?}",
                    defmt::Debug2Format(&error)
                );
            }
        }

        info!("Looping...");
        Timer::after(Duration::from_secs(60)).await;
    }
}

async fn fetch_time_from_ntp(stack: Stack<'_>, _tls_seed: u64, rtc: &Rtc<'_>) {
    let ntp_addrs = stack.dns_query(NTP_SERVER, DnsQueryType::A).await.unwrap();
    let addr: IpAddr = ntp_addrs[0].into();
    if ntp_addrs.is_empty() {
        panic!("Failed to resolve DNS. Empty result");
    }

    let mut rx_meta = [PacketMetadata::EMPTY; 16];
    let mut rx_buffer = [0; 4096];
    let mut tx_meta = [PacketMetadata::EMPTY; 16];
    let mut tx_buffer = [0; 4096];
    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    socket.bind(123).unwrap();

    let result = get_time(
        SocketAddr::from((addr, 123)),
        &socket,
        NtpContext::new(Timestamp {
            rtc,
            current_time_us: 0,
        }),
    )
    .await;

    match result {
        Ok(time) => {
            // Set time immediately after receiving to reduce time offset.
            debug!(
                "NTP time received: sec={}, frac={}",
                time.sec(),
                time.sec_fraction()
            );
            rtc.set_current_time_us(
                (time.sec() as u64 * USEC_IN_SEC)
                    + ((time.sec_fraction() as u64 * USEC_IN_SEC) >> 32),
            );
        }
        Err(e) => {
            defmt::error!("Error getting time: {}", defmt::Debug2Format(&e));
        }
    }
    flush();
}

async fn wait_for_connection(stack: Stack<'_>) -> embassy_net::Ipv4Cidr {
    info!("Waiting to get link...");
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
        debug!("waiting for network stack...");
    }

    info!("Requesting IP address from dhcp.");
    loop {
        if let Some(config) = stack.config_v4() {
            info!("Got IP: {}", config.address);
            break;
        }
        debug!("Waiting...");
        Timer::after(Duration::from_millis(500)).await;
    }
    flush();
    stack.config_v4().unwrap().address
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    info!("start connection task");
    info!("Device capabilities: {:?}", controller.capabilities());
    loop {
        if let WifiStaState::Connected = esp_radio::wifi::sta_state() {
            // wait until we're no longer connected
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            Timer::after(Duration::from_millis(5000)).await
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let station_config = ModeConfig::Client(
                ClientConfig::default()
                    .with_ssid(SSID.into())
                    .with_password(PASSWORD.into()),
            );
            controller.set_config(&station_config).unwrap();
            info!("Starting WiFi...");
            controller.start_async().await.unwrap();
            info!("Wifi started!");

            info!("Scan for access points...");
            let scan_type = esp_radio::wifi::ScanTypeConfig::Active {
                min: Duration::from_millis(100).into(),
                max: Duration::from_millis(300).into(),
            };
            let scan_config = ScanConfig::default().with_max(10).with_scan_type(scan_type);
            let result = controller
                .scan_with_config_async(scan_config)
                .await
                .unwrap();
            info!("Scan result: Found {} access points:", result.len());
            for ap in result {
                info!("{:?}", ap);
            }
        }
        info!("Attempting connection to {}...", SSID);

        match controller.connect_async().await {
            Ok(_) => info!("Wifi connected!"),
            Err(e) => {
                error!("Failed to connect to wifi: {}", defmt::Debug2Format(&e));
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}
