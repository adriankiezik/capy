use bevy_ecs::system::NonSendMut;

use crate::resources::{BlitPipeline, GpuContext, LightingPipeline};

pub(crate) fn resize_blit_system(
    gpu: NonSendMut<GpuContext>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    blit: Option<NonSendMut<BlitPipeline>>,
) {
    let (Some(lighting), Some(mut blit)) = (lighting, blit) else {
        return;
    };
    let needs_rebind = blit.width != gpu.config.width
        || blit.height != gpu.config.height
        || blit.source_width != lighting.width
        || blit.source_height != lighting.height;
    if needs_rebind {
        blit.rebuild_bind_group(&gpu.device, &lighting.output_color);
        blit.width = gpu.config.width;
        blit.height = gpu.config.height;
        blit.source_width = lighting.width;
        blit.source_height = lighting.height;
    }
}
