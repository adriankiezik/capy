mod config;

use capy_engine_server::{runtime::spawn_dedicated, session::SessionConfig};
use clap::Parser;
use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

fn main() -> anyhow::Result<()> {
    let config = config::Config::parse();

    let stopping = Arc::new(AtomicBool::new(false));

    #[cfg(unix)]
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, stopping.clone())?;
    }

    let session = capy_server::create(SessionConfig {
        tick_rate: config.tick_rate,
        max_peers: config.max_players,
        ..Default::default()
    })?;

    let server = spawn_dedicated(session, Default::default(), TcpListener::bind(config.bind)?)?;

    while !stopping.load(Ordering::Acquire) && !server.finished() {
        std::thread::sleep(Duration::from_millis(50));
    }

    server.shutdown();

    server.join()?;

    Ok(())
}
