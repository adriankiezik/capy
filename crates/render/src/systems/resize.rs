use bevy_ecs::system::{NonSendMut, Res};
use capy_core::GameWindow;

use crate::resources::GpuContext;

pub(crate) fn resize_surface_system(mut gpu: NonSendMut<GpuContext>, window: Res<GameWindow>) {
    if window.width > 0
        && window.height > 0
        && (gpu.config.width != window.width || gpu.config.height != window.height)
    {
        gpu.resize(window.width, window.height);
    }
}
