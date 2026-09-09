pub type Result<T> = std::result::Result<T, ReplicaError>;

#[derive(Debug, thiserror::Error)]
pub enum ReplicaError {
    #[error(transparent)]
    Mesh(#[from] crate::replica::mesh::MeshError),
    #[error("invalid replica state")]
    Invalid,
    #[error("invalid replica setting: {0}")]
    InvalidSetting(&'static str),
    #[error("replica capacity exceeded: {0}")]
    Limit(&'static str),
    #[error("replica revision does not match update")]
    RevisionMismatch,
}
