use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;

use crate::resources::{FrameInProgress, RenderOverlayCallbacks};

pub(crate) fn submit_frame_system(world: &mut World) -> Result<(), BevyError> {
    let Some(frame_data) = world.non_send_resource_mut::<FrameInProgress>().data.take() else {
        return Ok(());
    };

    let mut encoder = frame_data.encoder;
    let overlay_callbacks = world
        .get_resource::<RenderOverlayCallbacks>()
        .map(|callbacks| callbacks.list().to_vec())
        .unwrap_or_default();
    for callback in overlay_callbacks {
        callback(
            world,
            &frame_data.device,
            &frame_data.queue,
            frame_data.surface_format,
            &mut encoder,
            &frame_data.output_view,
        )?;
    }

    frame_data.queue.submit(std::iter::once(encoder.finish()));
    frame_data.output.present();
    Ok(())
}
