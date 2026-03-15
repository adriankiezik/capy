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
    if blit.width != gpu.config.width || blit.height != gpu.config.height {
        blit.rebuild_bind_group(&gpu.device, &lighting.output_color);
        blit.width = gpu.config.width;
        blit.height = gpu.config.height;
    }
}
