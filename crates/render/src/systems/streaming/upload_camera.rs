use bevy_ecs::system::{NonSendMut, Res};
use capy_core::Camera;

use crate::resources::{GpuContext, StreamingPipeline};

pub(crate) fn upload_camera_system(
    gpu: NonSendMut<GpuContext>,
    streaming: NonSendMut<StreamingPipeline>,
    camera: Res<Camera>,
) {
    streaming.upload_camera(&gpu.queue, &camera, gpu.config.width, gpu.config.height);
}
