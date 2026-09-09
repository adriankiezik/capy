pub type Result<T> = std::result::Result<T, ReplicaError>;

#[derive(Debug, thiserror::Error)]
pub enum ReplicaError {
    #[error("duplicate render instance ID {0}")]
    DuplicateRenderInstance(u64),
    #[error("render instance {id} has invalid {field}")]
    InvalidRenderInstance { id: u64, field: &'static str },
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
