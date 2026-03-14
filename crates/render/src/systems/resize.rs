use bevy_ecs::system::{NonSendMut, Res};
use capy_core::GameWindow;

use crate::resources::{BlitPipeline, GpuContext, StreamingPipeline};

pub(crate) fn resize_system(
    mut gpu: NonSendMut<GpuContext>,
    streaming: Option<NonSendMut<StreamingPipeline>>,
    blit: Option<NonSendMut<BlitPipeline>>,
    window: Res<GameWindow>,
) {
    let (Some(mut streaming), Some(mut blit)) = (streaming, blit) else {
        return;
    };
    if window.width > 0
        && window.height > 0
        && (gpu.config.width != window.width || gpu.config.height != window.height)
    {
        gpu.resize(window.width, window.height);
        streaming.resize(&gpu.device, window.width, window.height);
        blit.rebuild_bind_group(&gpu.device, &streaming.storage_texture);
    }
}
