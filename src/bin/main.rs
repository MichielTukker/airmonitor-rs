#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{delay::Delay, prelude::*};
// use esp_hal::timer::timg::TimerGroup;
use log::info;
extern crate alloc;

use airmonitor_rs::devices::display;

// pub struct MeasurementData {
//     temperature: f32,
//     humidity: f32,
//     pm25: f32,
//     pm10: f32,
// }

#[entry]
fn main() -> ! {
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::max();
        config
    });
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(72 * 1024);

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
    display.print("Hello world!");

    let delay = Delay::new();
    loop {
        delay.delay(1500.millis());
        if !status {
            status = true;
        } else {
            display.print_at(0, 16, "Hello from Rust!!!");
        }
        info!("Hello world!");
    }
}
