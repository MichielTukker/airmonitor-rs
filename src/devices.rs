pub mod display {

    use defmt::{debug, error, info, warn};
    use embedded_graphics::{
        mono_font::{MonoTextStyle, MonoTextStyleBuilder, ascii::FONT_6X10},
        pixelcolor::BinaryColor,
        prelude::*,
        text::{Baseline, Text},
    };
    use esp_hal::gpio::interconnect::PeripheralOutput;
    use esp_hal::i2c::master::Config;
    use esp_hal::i2c::master::I2c;
    use esp_hal::i2c::master::Instance;
    use ssd1306::{I2CDisplayInterface, Ssd1306, prelude::*};

    pub struct OledDisplay<'a> {
        display: Ssd1306<
            I2CInterface<I2c<'a, esp_hal::Blocking>>,
            DisplaySize128x64,
            ssd1306::mode::BufferedGraphicsMode<DisplaySize128x64>,
        >,
        text_style: MonoTextStyle<'a, BinaryColor>,
    }

    //TODO implement scrolling buffer (println style)
    impl<'a> OledDisplay<'a> {
        pub fn new(
            device: impl Instance + 'a,
            sda_pin: impl PeripheralOutput<'a>,
            scl_pin: impl PeripheralOutput<'a>,
        ) -> Self {
            let i2c_inf = I2c::new(device, Config::default())
                .expect("I2C")
                .with_sda(sda_pin)
                .with_scl(scl_pin);

            // our display sits at i2c address 0x3C
            let interface = I2CDisplayInterface::new(i2c_inf); // i2c interface with default address 0x3C 
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
    use embedded_dht_rs::dht22::Dht22;
    use esp_hal::{delay::Delay, gpio::Flex};

    pub struct SensorReading {
        pub temperature: f32,
        pub humidity: f32,
    }
    pub struct EnvironmentSensor<'a> {
        dht22: Dht22<Flex<'a>, Delay>,
    }
    impl<'a> EnvironmentSensor<'a> {
        pub fn new(mut dht22_pin: Flex<'a>) -> Self {
            dht22_pin.set_output_enable(true);
            dht22_pin.set_input_enable(true);
            dht22_pin.set_high();

            let dht22 = Dht22::new(dht22_pin, Delay::new());
            Self { dht22 }
        }

        pub fn read(&mut self) -> Result<SensorReading, &'static str> {
            match self.dht22.read() {
                Ok(reading) => Ok(SensorReading {
                    temperature: reading.temperature,
                    humidity: reading.humidity,
                }),
                Err(_) => Err("Failed to read from sensor"),
            }
        }
    }
}
