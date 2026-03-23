use bevy_ecs::error::BevyError;
use bevy_ecs::system::{NonSendMut, Res, ResMut};

use crate::resources::TemporalCameraState;
use crate::resources::trace::TracePipeline;
#[cfg(feature = "dlss")]
use crate::resources::{AoMode, DlssPipeline, DlssSettings, RendererSettings, RtaoPipeline};
use crate::resources::{
    BlitPipeline, FrameInProgress, FsrPipeline, FsrSettings, GpuContext, GpuProfiler, GtaoPipeline,
    LightingPipeline,
};

pub(crate) fn render_passes_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    blit: Option<NonSendMut<BlitPipeline>>,
    temporal: Res<TemporalCameraState>,
    #[cfg(feature = "dlss")] dlss: Option<NonSendMut<DlssPipeline>>,
    #[cfg(feature = "dlss")] mut dlss_settings: Option<ResMut<DlssSettings>>,
    #[cfg(feature = "dlss")] rtao: Option<NonSendMut<RtaoPipeline>>,
    #[cfg(feature = "dlss")] renderer_settings: Option<Res<RendererSettings>>,
    fsr: Option<NonSendMut<FsrPipeline>>,
    mut fsr_settings: Option<ResMut<FsrSettings>>,
    mut frame: NonSendMut<FrameInProgress>,
    mut gpu_profiler: NonSendMut<GpuProfiler>,
) -> Result<(), BevyError> {
    let (Some(mut trace), Some(gtao), Some(lighting), Some(blit)) = (trace, gtao, lighting, blit)
    else {
        return Ok(());
    };

    let output = match gpu.surface.get_current_texture() {
        Ok(output) => output,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            gpu.surface.configure(&gpu.device, &gpu.config);
            gpu.surface.get_current_texture()?
        }
        Err(wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Other) => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    let output_view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = frame.encoder.take().unwrap_or_else(|| {
        gpu.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            })
    });

    trace.clear_stats(&mut encoder);

    {
        let ts = gpu_profiler.pass_indices("trace");
        let ts_writes = ts.map(|(b, e)| wgpu::ComputePassTimestampWrites {
            query_set: gpu_profiler.query_set(),
            beginning_of_pass_write_index: Some(b),
            end_of_pass_write_index: Some(e),
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Trace Pass"),
            timestamp_writes: ts_writes,
        });
        pass.set_pipeline(&trace.compute_pipeline);
        pass.set_bind_group(0, &trace.compute_bind_group, &[]);
        pass.dispatch_workgroups(trace.width.div_ceil(8), trace.height.div_ceil(8), 1);
    }
    trace.copy_stats_to_readback(&mut encoder);

    #[cfg(feature = "dlss")]
    let use_rtao = renderer_settings
        .as_ref()
        .is_some_and(|s| s.ao_mode == AoMode::RayTraced && s.ao_intensity > 0.0)
        && rtao.is_some();

    #[cfg(feature = "dlss")]
    if use_rtao {
        if let Some(ref rtao) = rtao {
            rtao.update_params(
                &gpu.queue,
                renderer_settings.as_deref().unwrap_or(&Default::default()),
                temporal.frame_index(),
            );
            let ts = gpu_profiler.pass_indices("ao");
            let ts_writes = ts.map(|(b, e)| wgpu::ComputePassTimestampWrites {
                query_set: gpu_profiler.query_set(),
                beginning_of_pass_write_index: Some(b),
                end_of_pass_write_index: Some(e),
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("RTAO Pass"),
                timestamp_writes: ts_writes,
            });
            pass.set_pipeline(&rtao.compute_pipeline);
            pass.set_bind_group(0, &rtao.bind_group, &[]);
            pass.dispatch_workgroups(rtao.width.div_ceil(8), rtao.height.div_ceil(8), 1);
        }
    } else {
        let ts = gpu_profiler.pass_indices("ao");
        let ts_writes = ts.map(|(b, e)| wgpu::ComputePassTimestampWrites {
            query_set: gpu_profiler.query_set(),
            beginning_of_pass_write_index: Some(b),
            end_of_pass_write_index: Some(e),
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("GTAO Pass"),
            timestamp_writes: ts_writes,
        });
        pass.set_pipeline(&gtao.compute_pipeline);
        pass.set_bind_group(0, &gtao.bind_group, &[]);
        pass.dispatch_workgroups(gtao.width.div_ceil(8), gtao.height.div_ceil(8), 1);
    }

    #[cfg(not(feature = "dlss"))]
    {
        let ts = gpu_profiler.pass_indices("ao");
        let ts_writes = ts.map(|(b, e)| wgpu::ComputePassTimestampWrites {
            query_set: gpu_profiler.query_set(),
            beginning_of_pass_write_index: Some(b),
            end_of_pass_write_index: Some(e),
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("GTAO Pass"),
            timestamp_writes: ts_writes,
        });
        pass.set_pipeline(&gtao.compute_pipeline);
        pass.set_bind_group(0, &gtao.bind_group, &[]);
        pass.dispatch_workgroups(gtao.width.div_ceil(8), gtao.height.div_ceil(8), 1);
    }

    {
        let ts = gpu_profiler.pass_indices("lit");
        let ts_writes = ts.map(|(b, e)| wgpu::ComputePassTimestampWrites {
            query_set: gpu_profiler.query_set(),
            beginning_of_pass_write_index: Some(b),
            end_of_pass_write_index: Some(e),
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Lighting Pass"),
            timestamp_writes: ts_writes,
        });
        pass.set_pipeline(&lighting.compute_pipeline);
        pass.set_bind_group(0, &lighting.bind_group, &[]);
        pass.dispatch_workgroups(lighting.width.div_ceil(8), lighting.height.div_ceil(8), 1);
    }

    // Resolve GPU timestamp queries before the upscaler encoder split.
    gpu_profiler.resolve(&mut encoder);

    // Track whether an upscaler produced a command buffer (for the encoder split).
    let mut upscaler_cmd_buf: Option<wgpu::CommandBuffer> = None;

    #[cfg(feature = "dlss")]
    if upscaler_cmd_buf.is_none() {
        if let Some(mut dlss) = dlss {
            let reset = match dlss_settings.as_mut() {
                Some(settings) if settings.enabled && settings.reset => {
                    settings.reset = false;
                    true
                }
                _ => false,
            };
            let jitter = temporal.current_jitter();

            // Try Ray Reconstruction first, then Super Resolution
            let dlss_cmd_buf = if dlss.rr_output_texture().is_some() {
                dlss.render_ray_reconstruction(
                    &mut encoder,
                    &gpu.adapter,
                    &trace.gbuf_color.view,
                    &trace.gbuf_normal.view,
                    &lighting.output_color.view,
                    &trace.dlss_depth.view,
                    &trace.motion_vectors.view,
                    reset,
                    jitter,
                )?
            } else if dlss.output_texture().is_some() {
                dlss.render(
                    &mut encoder,
                    &gpu.adapter,
                    &lighting.output_color.view,
                    &trace.dlss_depth.view,
                    &trace.motion_vectors.view,
                    reset,
                    jitter,
                )?
            } else {
                None
            };

            upscaler_cmd_buf = dlss_cmd_buf;
        }
    }

    if upscaler_cmd_buf.is_none() {
        if let Some(mut fsr) = fsr {
            if fsr.output_texture().is_some() {
                let reset = match fsr_settings.as_mut() {
                    Some(settings) if settings.enabled && settings.reset => {
                        settings.reset = false;
                        true
                    }
                    _ => false,
                };
                let jitter = temporal.current_jitter();

                let fsr_cmd_buf = fsr.render(
                    &mut encoder,
                    &gpu.adapter,
                    &lighting.output_color.view,
                    &trace.dlss_depth.view,
                    &trace.motion_vectors.view,
                    reset,
                    jitter,
                    16.6, // TODO: pass actual frame delta time
                )?;
                upscaler_cmd_buf = fsr_cmd_buf;
            }
        }
    }

    if let Some(cmd_buf) = upscaler_cmd_buf {
        gpu.queue.submit([encoder.finish(), cmd_buf]);
        encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Post-Upscaler Encoder"),
            });
    }

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Blit Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
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

    frame.encoder = Some(encoder);
    frame.output = Some(output);
    frame.output_view = Some(output_view);

    Ok(())
}
