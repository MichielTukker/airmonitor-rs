pub mod wifi {
    use defmt::{debug, error, info};
    use embassy_net::{DhcpConfig, Runner, Stack, StackResources};

    use embassy_time::{Duration, Timer};

    use crate::mk_static;
    use esp_hal::{peripherals::WIFI, rng::Rng};
    use esp_radio::wifi::{
        ClientConfig, ModeConfig, ScanConfig, WifiController, WifiDevice, WifiEvent, WifiStaState,
    };

    pub fn init_network_stack<'a>(
        radio_init: &'a esp_radio::Controller<'a>,
        device: WIFI<'a>,
    ) -> (
        Stack<'a>,
        u64,
        WifiController<'a>,
        embassy_net::Runner<'a, WifiDevice<'a>>,
    ) {
        let (controller, interfaces) = esp_radio::wifi::new(radio_init, device, Default::default())
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

        (stack, tls_seed, controller, runner)
    }

    pub async fn wait_for_connection(stack: Stack<'_>) -> embassy_net::Ipv4Cidr {
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
        stack.config_v4().unwrap().address
    }

    #[embassy_executor::task]
    pub async fn connection(
        mut controller: WifiController<'static>,
        ssid: &'static str,
        password: &'static str,
    ) {
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
                        .with_ssid(ssid.into())
                        .with_password(password.into()),
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
            info!("Attempting connection to {}...", ssid);

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
    pub async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
        runner.run().await
    }
}
