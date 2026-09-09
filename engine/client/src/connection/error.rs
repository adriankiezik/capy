#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("server disconnected: {0}")]
    Closed(String),
    #[error(transparent)]
    Replica(#[from] crate::replica::ReplicaError),
    #[error(transparent)]
    Protocol(#[from] capy_engine_protocol::codec::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("command sequence exhausted")]
    Sequence,
}

pub type Result<T> = std::result::Result<T, Error>;
