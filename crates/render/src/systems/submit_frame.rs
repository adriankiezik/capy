use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;

use crate::resources::{FrameInProgress, GpuContext, RenderOverlayCallbacks};

pub(crate) fn submit_frame_system(world: &mut World) -> Result<(), BevyError> {
    let mut frame = world.non_send_resource_mut::<FrameInProgress>();
    let Some(mut encoder) = frame.encoder.take() else {
        return Ok(());
    };
    let output = frame.output.take();
    let output_view = frame.output_view.take();
    let post_submit = std::mem::take(&mut frame.post_submit);

    let gpu = world.non_send_resource::<GpuContext>();
    let device = gpu.device.clone();
    let queue = gpu.queue.clone();
    let surface_format = gpu.config.format;

    let mut first_error: Option<BevyError> = None;
    if let Some(ref view) = output_view {
        let overlay_callbacks = world
            .get_resource::<RenderOverlayCallbacks>()
            .map(|callbacks| callbacks.list().to_vec())
            .unwrap_or_default();
        for callback in overlay_callbacks {
            if let Err(e) = callback(world, &device, &queue, surface_format, &mut encoder, view) {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }

    queue.submit(std::iter::once(encoder.finish()));

    for ps in post_submit {
        if let Err(e) = ps(world) {
            if first_error.is_none() {
                first_error = Some(e);
            }
        }
    }

    if let Some(output) = output {
        output.present();
    }

    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
