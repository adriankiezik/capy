#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Session(#[from] crate::session::Error),
    #[error(transparent)]
    Protocol(#[from] capy_engine_protocol::codec::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("server thread panicked")]
    Thread,
    #[error("invalid server runtime configuration")]
    Configuration,
}

pub type Result<T> = std::result::Result<T, Error>;
