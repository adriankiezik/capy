use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum RenderError {
    #[error(transparent)]
    CreateSurface(#[from] wgpu::CreateSurfaceError),

    #[error(transparent)]
    RequestAdapter(#[from] wgpu::RequestAdapterError),

    #[error(transparent)]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    #[error("adapter does not support any surface formats")]
    InvalidAdapter,

    #[error(transparent)]
    Surface(#[from] wgpu::SurfaceError),
}

pub(crate) type Result<T> = std::result::Result<T, RenderError>;
