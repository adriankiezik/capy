use bevy_ecs::error::BevyError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("{0}")]
    System(BevyError),

    #[error(transparent)]
    EventLoop(#[from] winit::error::EventLoopError),

    #[error(transparent)]
    Window(#[from] winit::error::OsError),
}

impl From<BevyError> for EngineError {
    fn from(e: BevyError) -> Self {
        Self::System(e)
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;
