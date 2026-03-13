use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Render(#[from] capy_render::RenderError),

    #[error(transparent)]
    EventLoop(#[from] winit::error::EventLoopError),

    #[error(transparent)]
    Window(#[from] winit::error::OsError),
}

pub type Result<T> = std::result::Result<T, EngineError>;
