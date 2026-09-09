use clap::Parser;
use std::net::SocketAddr;

#[derive(Parser)]
pub struct Config {
    #[arg(long, default_value = "0.0.0.0:42420")]
    pub bind: SocketAddr,
    #[arg(long, default_value_t = 20)]
    pub tick_rate: u32,
    #[arg(long, default_value_t = 16)]
    pub max_players: usize,
}
