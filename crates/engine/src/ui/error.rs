use thiserror::Error;

#[derive(Debug, Error)]
pub enum UiError {
    #[error("UI dimensions, scale, or colors are invalid")]
    InvalidLayout,
    #[error("UI command or geometry limit exceeded")]
    ResourceLimit,
    #[error("UI layout failed")]
    Layout(#[from] taffy::TaffyError),
}
