use crate::graphics::GraphicsError;
use crate::runtime::WindowError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, RuntimeError>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RuntimeError {
    #[error("window operation failed")]
    Window(#[from] WindowError),
    #[error("graphics operation failed")]
    Graphics(#[from] GraphicsError),
    #[error("event loop failed")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("application initialization failed")]
    Initialize(#[source] anyhow::Error),
    #[error("application update failed")]
    Update(#[source] anyhow::Error),
    #[error("retry delay must be nonzero and fit a monotonic deadline")]
    InvalidRetryDelay,
}
