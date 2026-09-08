pub type Result<T> = std::result::Result<T, PlayerError>;

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("invalid player configuration or movement")]
    Invalid,
    #[error(transparent)]
    Scene(#[from] crate::scene::SceneError),
}
