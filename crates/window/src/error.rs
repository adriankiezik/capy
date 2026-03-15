use bevy_ecs::error::BevyError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WindowError {
    #[error(transparent)]
    EventLoop(#[from] winit::error::EventLoopError),

    #[error(transparent)]
    Window(#[from] winit::error::OsError),

    #[error("{0}")]
    Schedule(BevyError),
}

impl From<BevyError> for WindowError {
    fn from(e: BevyError) -> Self {
        Self::Schedule(e)
    }
}

pub type Result<T> = std::result::Result<T, WindowError>;
