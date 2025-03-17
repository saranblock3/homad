use clap::arg;
use clap::command;
use clap::value_parser;
use clap::Parser;
use lazy_static::lazy_static;

#[allow(non_snake_case)]
pub mod CONST {
    pub const UNSCHEDULED_PRIORITY_LEVELS: usize = 6;
    pub const UNSCHEDULED_PRIORITY_PARTITIONS: usize = 5;
    pub const SCHEDULED_PRIORITY_LEVELS: usize = 2;
    pub const PRIORITY_LEVEL_WIDTH: usize = 8;
    pub const MINIMUM_WORKLOAD_SAMPLE_SIZE: usize = 100;
}

#[derive(Parser)]
#[command(version, long_about = None)]
#[allow(non_snake_case)]
pub struct Config {
    /// Path to socket for applications to connect to
    #[arg(short, default_value = "/tmp/homa.sock")]
    pub SOCKET_PATH: String,
    /// Max message length
    #[arg(short, default_value_t = 524_288_000)]
    pub MESSAGE_MAX_LENGTH: u64,
    /// Max datagram payload length, must be less than 1400
    #[arg(short, default_value_t = 1400, value_parser = value_parser!(u16).range(..=1400))]
    pub DATAGRAM_PAYLOAD_LENGTH: u16,
    /// Max number of unscheduled datagrams allowed
    #[arg(short, default_value_t = 6)]
    pub UNSCHEDULED_DATAGRAM_LIMIT: usize,
    /// Regular imeout to wait before issuing resends
    #[arg(short, default_value_t = 15)]
    pub TIMEOUT: u64,
    /// Large timeout to wait before aborting scheduled message transmission
    #[arg(short = 'T', default_value_t = 10000)]
    pub LARGE_TIMEOUT: u64,
    /// Regular number of resends to issue
    #[arg(short, default_value_t = 5)]
    pub RESENDS: usize,
    /// Large number of resends to issue
    #[arg(short = 'R', default_value_t = 20)]
    pub LARGE_RESENDS: usize,
}

lazy_static! {
    pub static ref CONFIG: Config = Config::parse();
}
