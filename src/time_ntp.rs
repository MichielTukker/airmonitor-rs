pub mod ntp {
    use jiff;
    pub const TIMEZONE: jiff::tz::TimeZone = jiff::tz::get!("UTC");
    pub const NTP_SERVER: &str = "pool.ntp.org";
}

pub mod timestamp {
}
