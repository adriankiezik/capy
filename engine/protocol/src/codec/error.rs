#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("connection closed")]
    Closed,
    #[error("connection budget exceeded")]
    Limit,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Codec(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
