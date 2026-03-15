use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;

use crate::resources::{ComputePassCallbacks, FrameInProgress, GpuAccess};

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

    let mut encoder = world
        .non_send_resource_mut::<FrameInProgress>()
        .encoder
        .take()
        .unwrap_or_else(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            })
        });

    for cb in &callbacks {
        (cb.encode)(world, &device, &queue, &mut encoder)?;
    }

    let mut frame = world.non_send_resource_mut::<FrameInProgress>();
    frame.encoder = Some(encoder);
    for cb in &callbacks {
        if let Some(post_submit) = cb.post_submit {
            frame.post_submit.push(post_submit);
        }
    }

    Ok(())
}
