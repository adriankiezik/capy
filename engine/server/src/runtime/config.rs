use crate::session::SessionConfig;

#[derive(Clone, Copy, Debug)]
pub struct RuntimeConfig {
    pub max_catch_up_ticks: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_catch_up_ticks: 4,
        }
    }
}

pub(super) struct ServerConfig {
    pub bind: String,
    pub session: SessionConfig,
    pub runtime: RuntimeConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:42420".into(),
            session: SessionConfig::default(),
            runtime: RuntimeConfig::default(),
        }
    }
}
