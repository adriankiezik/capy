use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{GameWindow, WindowConfig};

use crate::resources::{FrameInProgress, GpuContext};

pub(crate) fn init_gpu(world: &mut World) -> Result<(), BevyError> {
    let window = world.resource::<GameWindow>();
    let handle = window.handle.clone();
    let width = window.width;
    let height = window.height;
    let vsync = world
        .get_resource::<WindowConfig>()
        .map(|c| c.vsync)
        .unwrap_or(true);

    let gpu = GpuContext::new(handle, width, height, vsync)?;
    world.insert_non_send_resource(gpu);
    world.insert_non_send_resource(FrameInProgress::empty());
    Ok(())
}
