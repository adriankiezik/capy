pub type Result<T> = std::result::Result<T, ScenarioError>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScenarioError {
    #[error("resolution must be positive")]
    ZeroResolution,
    #[error("resolution exceeds device limits")]
    ResolutionLimit,
    #[error("no GPU adapter available; select using WGPU_BACKEND and WGPU_ADAPTER_NAME")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("create scenario GPU device")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("GPU completion failed")]
    Poll(#[from] wgpu::PollError),
    #[error("timestamp mapping callback failed")]
    Callback(#[from] std::sync::mpsc::RecvTimeoutError),
    #[error("timestamp buffer mapping failed")]
    Map(#[from] wgpu::BufferAsyncError),
    #[error("reading timestamp buffer failed")]
    Readback(#[from] wgpu::MapRangeError),
    #[error("invalid timestamp buffer layout")]
    TimestampLayout(#[from] std::array::TryFromSliceError),
    #[error("invalid GPU timestamp duration")]
    InvalidTimestamp,
    #[error("preparing scenario frame failed")]
    Graphics(#[from] crate::graphics::GraphicsError),
}
