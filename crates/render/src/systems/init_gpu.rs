use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{GameWindow, WindowConfig};

#[cfg(feature = "dlss")]
use crate::resources::DlssSettings;
use crate::resources::{FrameInProgress, GpuAccess, GpuContext, GpuProfiler};

pub(crate) fn init_gpu(world: &mut World) -> Result<(), BevyError> {
    let window = world.resource::<GameWindow>();
    let handle = window.handle.clone();
    let width = window.width;
    let height = window.height;
    let vsync = world
        .get_resource::<WindowConfig>()
        .map(|c| c.vsync)
        .unwrap_or(true);
    #[cfg(feature = "dlss")]
    let dlss_project_id = world
        .get_resource::<DlssSettings>()
        .map(|settings| settings.project_id);

    let gpu = GpuContext::new(
        handle,
        width,
        height,
        vsync,
        #[cfg(feature = "dlss")]
        dlss_project_id,
    )?;
    let gpu_profiler = GpuProfiler::new(&gpu.device, &gpu.queue, gpu.timestamp_supported);
    world.insert_resource(GpuAccess {
        device: gpu.device.clone(),
        queue: gpu.queue.clone(),
    });
    world.insert_non_send_resource(gpu);
    world.insert_non_send_resource(gpu_profiler);
    world.insert_non_send_resource(FrameInProgress::empty());
    Ok(())
}
