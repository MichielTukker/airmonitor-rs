pub mod ntp {
    use core::net::{IpAddr, SocketAddr};
    use defmt::{debug, error, info};
    use embassy_net::{
        Stack,
        dns::DnsQueryType,
        udp::{PacketMetadata, UdpSocket},
    };
    use esp_hal::rtc_cntl::Rtc;
    // use jiff::Timestamp;
    use sntpc::{NtpContext, NtpTimestampGenerator, get_time};

    /// Microseconds in a second
    const USEC_IN_SEC: u64 = 1_000_000;

    #[derive(Clone, Copy)]
    pub struct NtpTimestamp<'a> {
        rtc: &'a Rtc<'a>,
        current_time_us: u64,
    }

    impl NtpTimestampGenerator for NtpTimestamp<'_> {
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

    pub async fn update_clock_from_ntp(
        stack: Stack<'_>,
        _tls_seed: u64,
        rtc: &Rtc<'_>,
        ntp_url: &str,
        timezone: &str,
    ) {
        let ntp_addrs = stack.dns_query(ntp_url, DnsQueryType::A).await.unwrap();
        let addr: IpAddr = ntp_addrs[0].into();
        if ntp_addrs.is_empty() {
            error!("Failed to resolve DNS. Empty result");
            // flush();
            return;
        }
        // info!("Querying NTP server {}", ntp_url);
        // info!("Querying NTP server {} at address {}", ntp_url, defmt::Debug2Format(&addr));

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
            NtpContext::new(NtpTimestamp {
                rtc,
                current_time_us: 0,
            }),
        )
        .await;
        info!(
            "NTP query completed, converting to local time: {}",
            defmt::Display2Format(&timezone)
        );

        match result {
            Ok(time) => {
                debug!(
                    "NTP time received: sec={}, frac={}",
                    time.sec(),
                    time.sec_fraction()
                );

                // Set time immediately after receiving to reduce time offset.
                rtc.set_current_time_us(
                    (time.sec() as u64 * USEC_IN_SEC)
                        + ((time.sec_fraction() as u64 * USEC_IN_SEC) >> 32),
                );

                // let ts = Timestamp::from_microsecond(rtc.current_time_us() as i64).unwrap();
                // let zoned_ts = ts.in_tz(timezone).unwrap().timestamp();
                // let ts_us = zoned_ts.as_microsecond() as u64;
                // rtc.set_current_time_us(ts_us);

                // info!(
                //     "Time in timezone {}: {}",
                //     timezone,
                //     defmt::Display2Format(&zoned_ts)
                // );
            }
            Err(e) => {
                error!("Error getting time: {}", defmt::Debug2Format(&e));
            }
        }
        let now = jiff::Timestamp::from_microsecond(rtc.current_time_us() as i64).unwrap();
        info!("Current time: {}", defmt::Display2Format(&now));
    }
}
