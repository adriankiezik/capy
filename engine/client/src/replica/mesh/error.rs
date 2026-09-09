#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    #[error("invalid mesh source")]
    Invalid,
    #[error("mesh budget exceeded: {0}")]
    Limit(&'static str),
    #[error("mesh worker failed")]
    Worker,
}

pub(crate) type Result<T> = std::result::Result<T, MeshError>;
