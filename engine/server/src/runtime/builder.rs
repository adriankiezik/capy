use super::{
    Error, Result, ServerHandle, config::ServerConfig, shutdown::Shutdown, spawn_dedicated,
    spawn_local,
};
use crate::{
    session::{Game, Session},
    simulation::Simulation,
};
use capy_engine_protocol::codec::Link;
use std::{net::TcpListener, time::Duration};

pub struct ServerBuilder<G: Game> {
    config: ServerConfig,
    initialize: Box<dyn FnOnce() -> anyhow::Result<(Simulation, G)>>,
}

impl<G: Game> ServerBuilder<G> {
    pub fn new(initialize: impl FnOnce() -> anyhow::Result<(Simulation, G)> + 'static) -> Self {
        Self {
            config: ServerConfig::default(),
            initialize: Box::new(initialize),
        }
    }

    pub fn with_bind(mut self, address: impl ToString) -> Self {
        self.config.bind = address.to_string();

        self
    }

    pub fn with_tick_rate(mut self, tick_rate: u32) -> Self {
        self.config.session.tick_rate = tick_rate;

        self
    }

    pub fn with_max_players(mut self, max_players: usize) -> Self {
        self.config.session.max_peers = max_players;

        self
    }

    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.config.session.connection_timeout = timeout;

        self
    }

    pub fn with_max_commands_per_tick(mut self, limit: usize) -> Self {
        self.config.session.max_commands_per_tick = limit;

        self
    }

    pub fn with_max_catch_up_ticks(mut self, limit: u32) -> Self {
        self.config.runtime.max_catch_up_ticks = limit;

        self
    }

    pub fn run(self) -> Result<()> {
        let shutdown = Shutdown::new()?;

        let server = self.spawn()?;

        while !shutdown.requested() && !server.finished() {
            std::thread::sleep(Duration::from_millis(50));
        }

        server.shutdown();

        server.join()
    }

    pub fn spawn(self) -> Result<ServerHandle> {
        self.validate()?;

        let listener = TcpListener::bind(&self.config.bind)?;

        self.host(listener)
    }

    pub fn host(self, listener: TcpListener) -> Result<ServerHandle> {
        let runtime = self.config.runtime;

        spawn_dedicated(self.session()?, runtime, listener)
    }

    pub fn local(self) -> Result<(Box<dyn Link>, ServerHandle)> {
        let runtime = self.config.runtime;

        spawn_local(self.session()?, runtime)
    }

    fn validate(&self) -> Result<()> {
        self.config.session.validate()?;

        if self.config.runtime.max_catch_up_ticks == 0 {
            return Err(Error::Configuration);
        }

        Ok(())
    }

    fn session(self) -> Result<Session<G>> {
        self.validate()?;

        let (simulation, game) = (self.initialize)().map_err(Error::Initialization)?;

        Ok(Session::new(self.config.session, simulation, game)?)
    }
}
