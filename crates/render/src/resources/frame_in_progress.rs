use crate::resources::ComputePassPostSubmit;

pub(crate) struct FrameInProgress {
    pub(crate) encoder: Option<wgpu::CommandEncoder>,
    pub(crate) output: Option<wgpu::SurfaceTexture>,
    pub(crate) output_view: Option<wgpu::TextureView>,
    pub(crate) post_submit: Vec<ComputePassPostSubmit>,
}

impl FrameInProgress {
    pub(crate) fn empty() -> Self {
        Self {
            encoder: None,
            output: None,
            output_view: None,
            post_submit: Vec::new(),
        }
    }
}
