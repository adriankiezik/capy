use clap::Parser;
use std::net::SocketAddr;

#[derive(Parser)]
pub struct Config {
    #[arg(long)]
    pub connect: Option<SocketAddr>,
}

pub const LOCAL_SERVER: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 42420);

pub fn presentation() -> engine::replica::ReplicaConfig {
    engine::replica::ReplicaConfig {
        material_colors: [
            (
                capy_engine_protocol::world::MaterialId(1),
                [0.29, 0.43, 0.24],
            ),
            (
                capy_engine_protocol::world::MaterialId(2),
                [0.78, 0.31, 0.13],
            ),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    }
}
