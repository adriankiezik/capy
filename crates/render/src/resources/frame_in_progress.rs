use crate::resources::ComputePassPostSubmit;

pub(crate) struct FrameInProgress {
    pub(crate) encoder: Option<wgpu::CommandEncoder>,
    pub(crate) output: Option<wgpu::SurfaceTexture>,
    pub(crate) output_view: Option<wgpu::TextureView>,
    pub(crate) post_submit: Vec<ComputePassPostSubmit>,
    /// When true, `output`/`output_view` contain the interpolated FG frame and
    /// `submit_frame_system` must present it first, then acquire a new
    /// swapchain texture and blit the real frame via `BlitPipeline`.
    pub(crate) fg_needs_real_blit: bool,
}

impl FrameInProgress {
    pub(crate) fn empty() -> Self {
        Self {
            encoder: None,
            output: None,
            output_view: None,
            post_submit: Vec::new(),
            fg_needs_real_blit: false,
        }
    }
}
