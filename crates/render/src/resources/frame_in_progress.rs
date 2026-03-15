pub(crate) struct FrameInProgress {
    pub(crate) data: Option<FrameData>,
}

pub(crate) struct FrameData {
    pub(crate) encoder: wgpu::CommandEncoder,
    pub(crate) output: wgpu::SurfaceTexture,
    pub(crate) output_view: wgpu::TextureView,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) surface_format: wgpu::TextureFormat,
}

impl FrameInProgress {
    pub(crate) fn empty() -> Self {
        Self { data: None }
    }
}
