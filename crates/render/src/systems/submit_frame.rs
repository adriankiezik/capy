use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::FrameProfiler;

#[cfg(feature = "dlss")]
use crate::resources::DlssSettings;
use crate::resources::FsrSettings;
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
    let fg_needs_real_blit = frame.fg_needs_real_blit;
    frame.fg_needs_real_blit = false;
    drop(frame);

    let gpu = world.non_send_resource::<GpuContext>();
    let device = gpu.device.clone();
    let queue = gpu.queue.clone();
    let surface_format = gpu.config.format;

    let mut first_error: Option<BevyError> = None;

    let overlay_callbacks = world
        .get_resource::<RenderOverlayCallbacks>()
        .map(|callbacks| callbacks.list().to_vec())
        .unwrap_or_default();

    if fg_needs_real_blit {
        // FG path: the current swapchain texture has the interpolated frame.
        // 1. Render egui on it and present.
        if let Some(ref view) = output_view {
            for callback in &overlay_callbacks {
                if let Err(e) = callback(world, &device, &queue, surface_format, &mut encoder, view)
                    && first_error.is_none()
                {
                    first_error = Some(e);
                }
            }
        }
        queue.submit(std::iter::once(encoder.finish()));

        if let Some(output) = output {
            output.present();
        }

        // 2. Acquire a new swapchain texture, blit the real frame, render
        //    egui, and present.  The real frame stays on screen between ticks.
        let gpu = world.non_send_resource::<GpuContext>();
        let new_output = match gpu.surface.get_current_texture() {
            Ok(o) => Some(o),
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                gpu.surface.get_current_texture().ok()
            }
            _ => None,
        };

        if let Some(new_output) = new_output {
            let new_view = new_output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let blit = world.non_send_resource::<crate::resources::BlitPipeline>();
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("FG Real Frame Encoder"),
            });
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("FG Real Blit Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &new_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                });
                pass.set_pipeline(&blit.blit_pipeline);
                pass.set_bind_group(0, &blit.blit_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            for callback in &overlay_callbacks {
                if let Err(e) =
                    callback(world, &device, &queue, surface_format, &mut enc, &new_view)
                    && first_error.is_none()
                {
                    first_error = Some(e);
                }
            }

            queue.submit(std::iter::once(enc.finish()));

            // Reflex: PRESENT markers around the real frame present.
            #[cfg(feature = "dlss")]
            {
                let gpu = world.non_send_resource::<GpuContext>();
                if let Some(reflex) = &gpu.reflex {
                    if let Some(sc) = crate::dlss::reflex::raw_swapchain(&gpu.surface) {
                        reflex.set_marker(sc, ash::vk::LatencyMarkerNV::PRESENT_START);
                    }
                }
            }

            new_output.present();

            #[cfg(feature = "dlss")]
            {
                let gpu = world.non_send_resource::<GpuContext>();
                if let Some(reflex) = &gpu.reflex {
                    if let Some(sc) = crate::dlss::reflex::raw_swapchain(&gpu.surface) {
                        reflex.set_marker(sc, ash::vk::LatencyMarkerNV::PRESENT_END);
                    }
                }
            }
        }
    } else {
        // Normal path (no FG): render egui, submit, present.
        if let Some(ref view) = output_view {
            for callback in &overlay_callbacks {
                if let Err(e) = callback(world, &device, &queue, surface_format, &mut encoder, view)
                    && first_error.is_none()
                {
                    first_error = Some(e);
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));

        // Reflex: RENDERSUBMIT_END after the final queue.submit.
        #[cfg(feature = "dlss")]
        {
            let gpu = world.non_send_resource::<GpuContext>();
            if let Some(reflex) = &gpu.reflex {
                if let Some(sc) = crate::dlss::reflex::raw_swapchain(&gpu.surface) {
                    reflex.set_marker(sc, ash::vk::LatencyMarkerNV::RENDERSUBMIT_END);
                }
            }
        }

        // Reflex: PRESENT markers around the real frame present.
        #[cfg(feature = "dlss")]
        {
            let gpu = world.non_send_resource::<GpuContext>();
            if let Some(reflex) = &gpu.reflex {
                if let Some(sc) = crate::dlss::reflex::raw_swapchain(&gpu.surface) {
                    reflex.set_marker(sc, ash::vk::LatencyMarkerNV::PRESENT_START);
                }
            }
        }

        if let Some(output) = output {
            output.present();
        }

        #[cfg(feature = "dlss")]
        {
            let gpu = world.non_send_resource::<GpuContext>();
            if let Some(reflex) = &gpu.reflex {
                if let Some(sc) = crate::dlss::reflex::raw_swapchain(&gpu.surface) {
                    reflex.set_marker(sc, ash::vk::LatencyMarkerNV::PRESENT_END);
                }
            }
        }
    }

    // Reflex: RENDERSUBMIT_END after the final queue.submit.
    #[cfg(feature = "dlss")]
    if fg_needs_real_blit {
        let gpu = world.non_send_resource::<GpuContext>();
        if let Some(reflex) = &gpu.reflex {
            if let Some(sc) = crate::dlss::reflex::raw_swapchain(&gpu.surface) {
                reflex.set_marker(sc, ash::vk::LatencyMarkerNV::RENDERSUBMIT_END);
            }
        }
    }

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

    if let Some(mut temporal) = world.get_resource_mut::<TemporalCameraState>() {
        temporal.finish_frame();
    }
    #[cfg(feature = "dlss")]
    if let Some(mut settings) = world.get_resource_mut::<DlssSettings>() {
        settings.reset = false;
    }
    if let Some(mut settings) = world.get_resource_mut::<FsrSettings>() {
        settings.reset = false;
    }

    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
