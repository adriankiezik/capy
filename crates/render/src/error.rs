use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error(transparent)]
    CreateSurface(#[from] wgpu::CreateSurfaceError),

    #[error(transparent)]
    RequestAdapter(#[from] wgpu::RequestAdapterError),

    #[error(transparent)]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    #[error("adapter does not support any surface formats")]
    InvalidAdapter,

    #[error(
        "voxel buffer '{label}' is {size} bytes, exceeding storage-buffer limits \
         (binding={max_storage_buffer_binding_size}, buffer={max_buffer_size})"
    )]
    BufferTooLarge {
        label: String,
        size: u64,
        max_storage_buffer_binding_size: u32,
        max_buffer_size: u64,
    },

    #[error(transparent)]
    Surface(#[from] wgpu::SurfaceError),

    #[cfg(feature = "dlss")]
    #[error(transparent)]
    Dlss(#[from] crate::dlss::DlssError),

    #[cfg(feature = "fsr")]
    #[error(transparent)]
    Fsr(#[from] crate::fsr::FsrError),
}

pub type Result<T> = std::result::Result<T, RenderError>;
