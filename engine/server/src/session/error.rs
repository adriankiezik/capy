#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid session configuration")]
    Configuration,
    #[error("session capacity exhausted")]
    Capacity,
    #[error("session counter exhausted")]
    Counter,
    #[error(transparent)]
    Protocol(#[from] capy_engine_protocol::codec::Error),
    #[error(transparent)]
    Simulation(#[from] crate::simulation::SimulationError),
    #[error(transparent)]
    Game(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
