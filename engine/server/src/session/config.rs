use capy_engine_protocol::codec::Limits;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub tick_rate: u32,
    pub max_peers: usize,
    pub max_commands_per_tick: usize,
    pub connection_timeout: Duration,
    pub wire: Limits,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            tick_rate: 20,
            max_peers: 16,
            max_commands_per_tick: 64,
            connection_timeout: Duration::from_secs(30),
            wire: Limits::default(),
        }
    }
}

impl SessionConfig {
    pub fn validate(&self) -> super::Result<()> {
        self.wire.validate()?;

        if !(1..=1000).contains(&self.tick_rate)
            || !(1..=4096).contains(&self.max_peers)
            || !(1..=1024).contains(&self.max_commands_per_tick)
            || self.connection_timeout.is_zero()
        {
            return Err(super::Error::Configuration);
        }

        Ok(())
    }
}
