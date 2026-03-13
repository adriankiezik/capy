use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error(transparent)]
    CreateSurface(#[from] wgpu::CreateSurfaceError),

    #[error(transparent)]
    RequestAdapter(#[from] wgpu::RequestAdapterError),

    #[error(transparent)]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    #[error(transparent)]
    Surface(#[from] wgpu::SurfaceError),
}

pub type Result<T> = std::result::Result<T, RenderError>;
