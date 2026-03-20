use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::FrameProfiler;

#[cfg(feature = "dlss")]
use crate::resources::DlssSettings;
use crate::resources::{
    FrameInProgress, GpuContext, GpuProfiler, RenderOverlayCallbacks, TemporalCameraState,
    TraceStatsReporter, trace::TracePipeline,
};

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
            if let Err(e) = callback(world, &device, &queue, surface_format, &mut encoder, view)
                && first_error.is_none()
            {
                first_error = Some(e);
            }
        }
    }

    queue.submit(std::iter::once(encoder.finish()));

    // Read back previous frame's GPU timestamps and feed into FrameProfiler.
    // Remove/reinsert to avoid double-borrow on World (FrameProfiler is Send, GpuProfiler is !Send).
    if let Some(mut profiler) = world.remove_resource::<FrameProfiler>() {
        {
            let gpu_profiler = world.non_send_resource::<GpuProfiler>();
            gpu_profiler.read_back(&device, &mut profiler);
        }
        world.insert_resource(profiler);
    }

    let trace_snapshot = world
        .get_non_send_resource::<TracePipeline>()
        .and_then(|trace| trace.read_back_stats(&device));
    if let (Some(snapshot), Some(mut reporter)) = (
        trace_snapshot,
        world.get_resource_mut::<TraceStatsReporter>(),
    ) {
        reporter.record(snapshot);
    }
    {
        let mut gpu_profiler = world.non_send_resource_mut::<GpuProfiler>();
        gpu_profiler.end_frame();
    }
    if let Some(mut trace) = world.get_non_send_resource_mut::<TracePipeline>() {
        trace.end_frame();
    }

    for ps in post_submit {
        if let Err(e) = ps(world)
            && first_error.is_none()
        {
            first_error = Some(e);
        }
    }

    if let Some(output) = output {
        output.present();
    }

    if let Some(mut temporal) = world.get_resource_mut::<TemporalCameraState>() {
        temporal.finish_frame();
    }
    #[cfg(feature = "dlss")]
    if let Some(mut settings) = world.get_resource_mut::<DlssSettings>() {
        settings.reset = false;
    }

    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
