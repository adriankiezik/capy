use thiserror::Error;

pub type Result<T> = std::result::Result<T, GraphicsError>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GraphicsError {
    #[error("creating the graphics surface failed")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("selecting a graphics adapter failed")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("creating the graphics device failed")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("required graphics features are unavailable: {0:?}")]
    MissingFeatures(wgpu::Features),
    #[error("surface has no supported configuration")]
    UnsupportedSurface,
    #[error("graphics presentation is suspended")]
    NotPresenting,
    #[error("surface size {width} x {height} exceeds maximum dimension {maximum}")]
    SurfaceSize {
        width: u32,
        height: u32,
        maximum: u32,
    },
    #[error("unsupported surface format: {0:?}")]
    SurfaceFormat(wgpu::TextureFormat),
    #[error("unsupported present mode: {0:?}")]
    PresentMode(wgpu::PresentMode),
    #[error("unsupported surface alpha mode: {0:?}")]
    AlphaMode(wgpu::CompositeAlphaMode),
    #[error("unsupported surface usages: {0:?}")]
    SurfaceUsage(wgpu::TextureUsages),
    #[error("maximum frame latency must be nonzero")]
    ZeroFrameLatency,
    #[error("GPU surface validation failed")]
    SurfaceValidation,
    #[error("GPU operation failed")]
    Gpu(#[from] wgpu::Error),
    #[error("application drawing failed")]
    Draw(#[source] anyhow::Error),
}
