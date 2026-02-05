# airmonitor-rs

[![Continuous Integration](https://github.com/MichielTukker/airmonitor-rs/actions/workflows/rust_ci.yml/badge.svg)](https://github.com/MichielTukker/airmonitor-rs/actions/workflows/rust_ci.yml)

esp32-based air monitor in rust

## Hardware

This runs on a ESP32 board with an ESP32-wroom-32 microcontroller (Xtensa).
The device displays the measured values on a display and connects to a wifi network to send MQTT messages with the measurement data.

## Roadmap

Feature list:

- [x] Working on esp32-wroom-32
- [x] Oled display
- [x] Working wifi
- [x] temperature/humidity sensor (DHT22)
- [ ] temperature/humidity sensor (BME280)
- [ ] dust sensor (SDS011)
- [ ] MQTT messaging
- [ ] Oled display refreshing
- [ ] Oled display scrolling buffer
