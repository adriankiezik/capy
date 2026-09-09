pub type Result<T> = std::result::Result<T, CharacterError>;

#[derive(Debug, thiserror::Error)]
pub enum CharacterError {
    #[error("invalid player configuration or movement")]
    Invalid,
    #[error(transparent)]
    Simulation(#[from] crate::simulation::SimulationError),
}
