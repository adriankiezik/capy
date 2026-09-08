pub type Result<T> = std::result::Result<T, SceneError>;

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error(transparent)]
    World(#[from] crate::world::WorldError),
    #[error("invalid scene or query settings")]
    Invalid,
    #[error("invalid scene setting: {0}")]
    InvalidSetting(&'static str),
    #[error("scene capacity or work limit exceeded: {0}")]
    Limit(&'static str),
}
