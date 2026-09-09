pub type Result<T> = std::result::Result<T, ModelError>;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model voxel dimensions must be between 1 and 4096 on each axis, got {0}")]
    InvalidDimensions(glam::IVec3),
    #[error("model leaf edge must be 8, 16, 32 or 64 voxels, got {0}")]
    InvalidLeafEdge(i32),
    #[error("model vertex budget must be greater than zero")]
    EmptyVertexBudget,
    #[error("model palette has {0} entries; at most 65535 are supported")]
    PaletteTooLarge(usize),
    #[error("model palette[{index}][{channel}] must be finite and between 0 and 1, got {value}")]
    InvalidColor {
        index: usize,
        channel: usize,
        value: f32,
    },
    #[error(
        "voxel {position} references palette index {index}, but the palette has {palette_len} entries"
    )]
    UndefinedMaterial {
        position: glam::IVec3,
        index: usize,
        palette_len: usize,
    },
    #[error("model capacity exceeded: {0}")]
    Limit(&'static str),
    #[error(transparent)]
    Mesh(#[from] crate::replica::MeshError),
}
