#![no_std]
#![no_main]

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::delay::Delay;
use esp_hal::{
    clock::CpuClock,
    gpio::{DriveMode, Flex, OutputConfig, Pull},
    rtc_cntl::Rtc,
    timer::timg::TimerGroup,
};

use embedded_dht_rs::dht22::Dht22;
use esp_println as _; // required to setup defmt global logger

extern crate alloc;
use alloc::format;

use airmonitor_rs::devices::display;
use airmonitor_rs::mk_static;
use airmonitor_rs::network::wifi::{
    connection, init_network_stack, net_task, wait_for_connection,
};
use airmonitor_rs::ntp_service::ntp::update_clock_from_ntp;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

const SSID: &str = env!("SSID", "SSID not set");
const PASSWORD: &str = env!("PASSWORD", "PASSWORD not set");

const NTP_URL: &str = match option_env!("NTP_URL") {
    Some(val) => val,
    None => "pool.ntp.org",
};

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
    let radio_init: &'static esp_radio::Controller<'static> = &*mk_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
    );
    // let (controller, interfaces) =
    //     esp_radio::wifi::new(radio_init, peripherals.WIFI, Default::default())
    //         .expect("Failed to initialize Wi-Fi controller");

    // let wifi_interface = interfaces.sta;
    // let rng = Rng::new();
    // let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);
    // let tls_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

    // let dhcp_config = DhcpConfig::default();
    // let config = embassy_net::Config::dhcpv4(dhcp_config);

    // // Init network stack
    // let (stack, runner) = embassy_net::new(
    //     wifi_interface,
    //     config,
    //     mk_static!(StackResources<3>, StackResources::<3>::new()),
    //     net_seed,
    // );

    let (stack, tls_seed, controller, runner) = init_network_stack(radio_init, peripherals.WIFI);
    spawner.spawn(connection(controller, SSID, PASSWORD)).ok();
    spawner.spawn(net_task(runner)).ok();

    let address = wait_for_connection(stack).await;

    display.print_at(0, 15, "v0.0.1.");
    display.print_at(0, 28, &format!("IP: {}", address));

    //TODO sensor readings (DHT/BME280 and SDS011)
    //TODO add pm2.5 sensor (SDS011)
    //TODO implement screen refreshing (clearing pixels before writing new text)
    //TODO add MQTT messaging

    update_clock_from_ntp(stack, tls_seed, &rtc, NTP_URL).await;
    let now = jiff::Timestamp::from_microsecond(rtc.current_time_us() as i64).unwrap();
    // let formatted_ts = now.to_string();
    info!("Current time: {}", defmt::Display2Format(&now));

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
