pub type Result<T> = std::result::Result<T, SimulationError>;

#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error(transparent)]
    World(#[from] crate::world::WorldError),
    #[error("invalid scene or query settings")]
    Invalid,
    #[error("invalid scene setting: {0}")]
    InvalidSetting(&'static str),
    #[error("scene capacity or work limit exceeded: {0}")]
    Limit(&'static str),
    #[error("scene geometry changed while preparing an edit")]
    StaleEdit,
    #[error("could not start scene edit worker: {0}")]
    EditWorker(std::io::Error),
    #[error("scene edit worker stopped without a result")]
    EditWorkerStopped,
}
