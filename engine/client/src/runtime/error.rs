use crate::graphics::GraphicsError;
use crate::runtime::WindowError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, RuntimeError>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RuntimeError {
    #[error("asset initialization failed")]
    Assets(#[from] crate::assets::AssetError),
    #[error("window operation failed")]
    Window(#[from] WindowError),
    #[error("graphics operation failed")]
    Graphics(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("event loop failed")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("application failed")]
    Application(#[source] anyhow::Error),
    #[error("engine is not running")]
    NotRunning,
    #[error("retry delay must be nonzero and fit a monotonic deadline")]
    InvalidRetryDelay,
    #[error("tick rate must be between 1 and 1000 Hz and catch-up limit must be nonzero")]
    InvalidTickRate,
}

impl From<GraphicsError> for RuntimeError {
    fn from(error: GraphicsError) -> Self {
        Self::Graphics(Box::new(error))
    }
}
