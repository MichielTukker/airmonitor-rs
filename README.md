# airmonitor-rs

[![Continuous Integration](https://github.com/MichielTukker/airmonitor-rs/actions/workflows/rust_ci.yml/badge.svg)](https://github.com/MichielTukker/airmonitor-rs/actions/workflows/rust_ci.yml)

esp32-based air monitor in rust

## Hardware

This runs on a ESP32 board with an ESP32-wroom-32 microcontroller (Xtensa).
The device displays the measured values on a display and connects to a wifi network to send MQTT messages with the measurement data.