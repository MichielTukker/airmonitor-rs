pub mod display {

    use embedded_graphics::{
        mono_font::{ascii::FONT_6X10, MonoTextStyle, MonoTextStyleBuilder},
        pixelcolor::BinaryColor,
        prelude::*,
        text::{Baseline, Text},
    };
    use esp_hal::i2c::master::Config;
    use esp_hal::i2c::master::I2c;
    use esp_hal::i2c::master::Instance;
    use esp_hal::{gpio::interconnect::PeripheralOutput, peripheral::Peripheral};
    use log::{debug, error, info, warn};
    use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
    pub struct OledDisplay<'a> {
        display: Ssd1306<
            I2CInterface<I2c<'a, esp_hal::Blocking>>,
            DisplaySize128x64,
            ssd1306::mode::BufferedGraphicsMode<DisplaySize128x64>,
        >,
        text_style: MonoTextStyle<'a, BinaryColor>,
    }
    impl<'a> OledDisplay<'a> {
        pub fn new(
            device: impl Peripheral<P = impl Instance> + 'a,
            sda_pin: impl Peripheral<P = impl PeripheralOutput> + 'a,
            scl_pin: impl Peripheral<P = impl PeripheralOutput> + 'a,
        ) -> Self {
            let i2c_inf: I2c<'_, esp_hal::Blocking> = I2c::new(device, Config::default())
                .with_sda(sda_pin)
                .with_scl(scl_pin);

            let interface = I2CDisplayInterface::new(i2c_inf);
            let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
                .into_buffered_graphics_mode();
            display.init().unwrap();

            let text_style = MonoTextStyleBuilder::new()
                .font(&FONT_6X10)
                .text_color(BinaryColor::On)
                .build();

            display.clear(BinaryColor::Off).unwrap();
            display.flush().unwrap();
            info!("Display initialized");

            Self {
                display,
                text_style,
            }
        }

        pub fn flush(&mut self) {
            let result = self.display.flush();
            match result {
                Ok(_) => (),
                Err(_) => error!("Failed to flush display"),
            }
        }

        pub fn clear(&mut self) {
            let result = self.display.clear(BinaryColor::Off);
            match result {
                Ok(_) => (),
                Err(_) => warn!("Failed to clear display"),
            }
        }

        pub fn print_at(&mut self, x: i32, y: i32, text: &str) {
            let result =
                Text::with_baseline(text, Point::new(x, y), self.text_style, Baseline::Top)
                    .draw(&mut self.display);
            match result {
                Ok(_) => (),
                Err(_) => debug!("Failed to print text to display"),
            }
            self.display.flush().unwrap();
        }

        pub fn print(&mut self, text: &str) {
            self.print_at(0, 0, text);
        }
    }
}

pub mod environment {
    pub mod dht11 {
        pub fn init() {}

        pub fn read() -> (f32, f32) {
            (0.0, 0.0)
        }
    }
}
