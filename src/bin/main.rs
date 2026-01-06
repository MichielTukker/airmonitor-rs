#![no_std]
#![no_main]

use defmt::{info};
use esp_hal::{
    clock::CpuClock,
    rng::Rng, timer::timg::TimerGroup,
};
use embassy_executor::Spawner;
use embassy_net::{
    DhcpConfig, Runner, Stack, StackResources,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_radio::{
    wifi::{
        ClientConfig, ModeConfig, ScanConfig, WifiController, WifiDevice, WifiEvent, WifiStaState,
    },
};
use reqwless::client::{HttpClient, TlsConfig};
extern crate alloc;
use alloc::{format};

use airmonitor_rs::devices::display;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 98767);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    // let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    //let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    esp_rtos::start(timg0.timer0);
    info!("Embassy initialized!");

    // static mut SECOND_CORE_STACK: esp_hal::system::Stack<{ 16 * 1024 }> =
    //     esp_hal::system::Stack::new();
    // esp_rtos::start_second_core(
    //     peripherals.CPU_CTRL,
    //     sw_int.software_interrupt0,
    //     sw_int.software_interrupt1,
    //     unsafe { &mut SECOND_CORE_STACK },
    //     || {},
    // );

    let esp_radio_controller = &*mk_static!(
        esp_radio::Controller<'static>, 
        esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
    );
    
    let mut display =
    display::OledDisplay::new(peripherals.I2C0, peripherals.GPIO5, peripherals.GPIO4);
    display.clear();
    display.print("=== Airmonitor-rs ===");

    // Setup wifi
    let (controller, interfaces) =
        esp_radio::wifi::new(&esp_radio_controller, peripherals.WIFI, Default::default()).expect("Failed to initialize Wi-Fi controller");

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

    access_website(stack, tls_seed).await;
    loop {}
}


async fn wait_for_connection(stack: Stack<'_>) -> embassy_net::Ipv4Cidr{
    println!("Waiting to get link...");
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
        println!("waiting for network stack...");
    }

    loop {
        println!("Waiting to get IP address...");
        if let Some(config) = stack.config_v4() {
            println!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    let ip_address = stack.config_v4().unwrap().address;
    ip_address
}

async fn access_website(stack: Stack<'_>, tls_seed: u64) {
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let dns = DnsSocket::new(stack);
    let tcp_state = TcpClientState::<1, 4096, 4096>::new();
    let tcp = TcpClient::new(stack, &tcp_state);

    let tls = TlsConfig::new(
        tls_seed,
        &mut rx_buffer,
        &mut tx_buffer,
        reqwless::client::TlsVerify::None,
    );

    let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
    let mut buffer = [0u8; 4096];
    let mut http_req = client
        .request(
            reqwless::request::Method::GET,
            "https://jsonplaceholder.typicode.com/posts/1",
        )
        .await
        .unwrap();
    let response = http_req.send(&mut buffer).await.unwrap();

    info!("Got response");
    let res = response.body().read_to_end().await.unwrap();

    let content = core::str::from_utf8(res).unwrap();
    println!("{}", content);
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());
    loop {
        match esp_radio::wifi::sta_state() {
            WifiStaState::Connected => {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let station_config = ModeConfig::Client(
                ClientConfig::default()
                    .with_ssid(SSID.into())
                    .with_password(PASSWORD.into()),
            );
            controller.set_config(&station_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");

            println!("Scan");
            let scan_type = esp_radio::wifi::ScanTypeConfig::Active {
                min: Duration::from_millis(100).into(),
                max: Duration::from_millis(300).into(),
            };
            let scan_config = ScanConfig::default().with_max(10).with_scan_type(scan_type);
            let result = controller
                .scan_with_config_async(scan_config)
                .await
                .unwrap();
            println!("Scan result: Found {} access points:", result.len());
            for ap in result {
                println!("{:?}", ap);
            }
        }
        println!("Attempting connection to {}...", SSID);

        match controller.connect_async().await {
            Ok(_) => println!("Wifi connected!"),
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}
