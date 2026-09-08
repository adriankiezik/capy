use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WindowError {
    #[error("creating the window failed")]
    Create(#[from] winit::error::OsError),
}
