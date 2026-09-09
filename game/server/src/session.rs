use crate::{CapyGame, world};
use capy_engine_protocol::codec::Link;
use capy_engine_server::{
    runtime::{ServerHandle, spawn_dedicated, spawn_local},
    session::{Session, SessionConfig},
};

pub fn create(config: SessionConfig) -> anyhow::Result<Session<CapyGame>> {
    Ok(Session::new(config, world::create()?, CapyGame::default())?)
}

pub fn host(listener: std::net::TcpListener) -> anyhow::Result<ServerHandle> {
    Ok(spawn_dedicated(
        create(SessionConfig::default())?,
        Default::default(),
        listener,
    )?)
}

pub fn local() -> anyhow::Result<(Box<dyn Link>, ServerHandle)> {
    Ok(spawn_local(
        create(SessionConfig::default())?,
        Default::default(),
    )?)
}
