use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;

use crate::resources::{ComputePassCallbacks, GpuAccess};

pub(crate) fn run_compute_passes(world: &mut World) -> Result<(), BevyError> {
    let callbacks = world
        .get_resource::<ComputePassCallbacks>()
        .map(|c| c.list().to_vec())
        .unwrap_or_default();

    if callbacks.is_empty() {
        return Ok(());
    }

    let Some(gpu) = world.get_resource::<GpuAccess>() else {
        return Ok(());
    };
    let device = gpu.device.clone();
    let queue = gpu.queue.clone();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Compute Passes Encoder"),
    });

    for cb in &callbacks {
        (cb.encode)(world, &device, &queue, &mut encoder)?;
    }

    queue.submit(std::iter::once(encoder.finish()));

    for cb in &callbacks {
        if let Some(post_submit) = cb.post_submit {
            post_submit(world)?;
        }
    }

    Ok(())
}
