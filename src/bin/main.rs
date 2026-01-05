#![no_std]
#![no_main]

use embedded_hal::delay::DelayNs;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, delay::Delay, main};
use log::info;
extern crate alloc;

use airmonitor_rs::devices::display;

// pub struct MeasurementData {
//     temperature: f32,
//     humidity: f32,
//     pm25: f32,
//     pm10: f32,
// }

//#[panic_handler]
//fn panic(_: &core::panic::PanicInfo) -> ! {
//    loop {}
//}

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init({
        let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
        config
    });
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(size: 32 * 1024);

    // Wifi initialization, not needed for now
    // let timg0 = TimerGroup::new(peripherals.TIMG0);
    // let _init = esp_wifi::init(
    //     timg0.timer0,
    //     esp_hal::rng::Rng::new(peripherals.RNG),
    //     peripherals.RADIO_CLK,
    // ).unwrap();

    let mut display =
        display::OledDisplay::new(peripherals.I2C0, peripherals.GPIO5, peripherals.GPIO4);
    let mut status = false;
    display.print("=== Airmonitor-rs ===");

    let mut delay = Delay::new();
    loop {
        delay.delay_ms(1500 as u32);
        if !status {
            status = true;
        } else {
            display.print_at(0, 16, "Hello from Rust!!!");
        }
        info!("Hello world!");
    }
}
