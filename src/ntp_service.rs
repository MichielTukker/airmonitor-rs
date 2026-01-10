pub mod ntp {
    use core::net::{IpAddr, SocketAddr};
    use defmt::{debug, flush};
    use embassy_net::{
        Stack,
        dns::DnsQueryType,
        udp::{PacketMetadata, UdpSocket},
    };
    use esp_hal::rtc_cntl::Rtc;
    use jiff;
    use sntpc::{NtpContext, NtpTimestampGenerator, get_time};

    pub const TIMEZONE: jiff::tz::TimeZone = jiff::tz::get!("UTC");
    pub const NTP_SERVER: &str = "pool.ntp.org";

    /// Microseconds in a second
    const USEC_IN_SEC: u64 = 1_000_000;

    #[derive(Clone, Copy)]
    pub struct Timestamp<'a> {
        rtc: &'a Rtc<'a>,
        current_time_us: u64,
    }

    impl NtpTimestampGenerator for Timestamp<'_> {
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
    ) {
        let ntp_addrs = stack.dns_query(ntp_url, DnsQueryType::A).await.unwrap();
        let addr: IpAddr = ntp_addrs[0].into();
        if ntp_addrs.is_empty() {
            panic!("Failed to resolve DNS. Empty result");
        }

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
            NtpContext::new(Timestamp {
                rtc,
                current_time_us: 0,
            }),
        )
        .await;

        match result {
            Ok(time) => {
                // Set time immediately after receiving to reduce time offset.
                debug!(
                    "NTP time received: sec={}, frac={}",
                    time.sec(),
                    time.sec_fraction()
                );
                rtc.set_current_time_us(
                    (time.sec() as u64 * USEC_IN_SEC)
                        + ((time.sec_fraction() as u64 * USEC_IN_SEC) >> 32),
                );
            }
            Err(e) => {
                defmt::error!("Error getting time: {}", defmt::Debug2Format(&e));
            }
        }
        flush();
    }
}
