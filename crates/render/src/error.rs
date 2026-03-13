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

    #[error(transparent)]
    Surface(#[from] SurfaceError),
}

impl From<wgpu::SurfaceError> for RenderError {
    fn from(e: wgpu::SurfaceError) -> Self {
        Self::Surface(e.into())
    }
}

#[derive(Debug, Error)]
pub enum SurfaceError {
    #[error("surface lost")]
    Lost,
    #[error("surface outdated")]
    Outdated,
    #[error("surface timed out")]
    Timeout,
    #[error("GPU out of memory")]
    OutOfMemory,
    #[error("other surface error")]
    Other,
}

impl From<wgpu::SurfaceError> for SurfaceError {
    fn from(e: wgpu::SurfaceError) -> Self {
        match e {
            wgpu::SurfaceError::Lost => Self::Lost,
            wgpu::SurfaceError::Outdated => Self::Outdated,
            wgpu::SurfaceError::Timeout => Self::Timeout,
            wgpu::SurfaceError::OutOfMemory => Self::OutOfMemory,
            wgpu::SurfaceError::Other => Self::Other,
        }
    }
}

pub type Result<T> = std::result::Result<T, RenderError>;
