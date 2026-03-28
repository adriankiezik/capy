use bevy_ecs::error::BevyError;
use bevy_ecs::system::{NonSendMut, Res, ResMut};

use crate::resources::TemporalCameraState;
use crate::resources::{
    BlitPipeline, FrameInProgress, GpuContext, GpuProfiler, GtaoPipeline, LightingPipeline,
};
#[cfg(feature = "dlss")]
use crate::resources::{DlssPipeline, DlssSettings};
#[cfg(feature = "fsr")]
use crate::resources::{FsrPipeline, FsrSettings};
use crate::resources::{RendererSettings, trace::TracePipeline};
use crate::settings::TonemappingUniform;

pub(crate) fn render_passes_system(
    #[cfg_attr(not(feature = "dlss"), allow(unused_mut))] mut gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    blit: Option<NonSendMut<BlitPipeline>>,
    temporal: Res<TemporalCameraState>,
    camera: Res<capy_core::Camera>,
    #[cfg(feature = "dlss")] dlss: Option<NonSendMut<DlssPipeline>>,
    #[cfg(feature = "dlss")] mut dlss_settings: Option<ResMut<DlssSettings>>,
    #[cfg(feature = "fsr")] fsr: Option<NonSendMut<FsrPipeline>>,
    #[cfg(feature = "fsr")] mut fsr_settings: Option<ResMut<FsrSettings>>,
    renderer_settings: Res<RendererSettings>,
    mut frame: NonSendMut<FrameInProgress>,
    mut gpu_profiler: NonSendMut<GpuProfiler>,
) -> Result<(), BevyError> {
    tracing::debug!("render_passes_system: enter");
    let (Some(mut trace), Some(gtao), Some(lighting), Some(blit)) = (trace, gtao, lighting, blit)
    else {
        tracing::debug!("render_passes_system: missing pipeline resources, skipping");
        return Ok(());
    };

    // --- Reflex: sleep to pace CPU, then begin a new frame -----------------
    #[cfg(feature = "dlss")]
    {
        let sc = crate::dlss::reflex::raw_swapchain(&gpu.surface);
        if let (Some(reflex), Some(sc)) = (&mut gpu.reflex, sc) {
            reflex.begin_frame();
            reflex.sleep(sc);
            reflex.set_marker(sc, ash::vk::LatencyMarkerNV::RENDERSUBMIT_START);
        }
    }

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
    #[allow(unused_mut)]
    let mut upscaler_cmd_buf: Option<wgpu::CommandBuffer> = None;

    #[cfg(feature = "dlss")]
    if let Some(mut dlss) = dlss {
        if upscaler_cmd_buf.is_none() {
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

        // Encoder split for upscaler output.
        if let Some(cmd_buf) = upscaler_cmd_buf.take() {
            gpu.queue.submit([encoder.finish(), cmd_buf]);
            encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Post-Upscaler Encoder"),
                });
        }

        // --- Frame Generation: evaluate + present interpolated FIRST ----------
        //
        // Present the interpolated frame first, then the real frame (with egui
        // applied later in submit_frame_system) is presented last and stays on
        // screen between ticks.
        if dlss.fg_output().is_some() {
            let fg_reset = dlss_settings.as_ref().is_some_and(|s| s.reset);

            let jitter = temporal.current_jitter();
            let clip_from_world = crate::camera::clip_from_world(&camera);
            let clip_from_world_arr = clip_from_world.to_cols_array();
            let prev_clip_from_world = temporal.previous_clip_from_world(clip_from_world_arr);
            let proj = glam::Mat4::perspective_infinite_rh(camera.fov_y, camera.aspect, 0.1);
            let fwd = camera.forward();
            let right = camera.right();
            let up = right.cross(fwd);

            let fg_camera = crate::dlss::frame_generation::FgCameraParams {
                view_to_clip: proj.to_cols_array(),
                clip_from_world: clip_from_world_arr,
                prev_clip_from_world,
                position: camera.position.to_array(),
                forward: fwd.to_array(),
                up: up.to_array(),
                right: right.to_array(),
                near: 0.1,
                fov_y: camera.fov_y,
                aspect: camera.aspect,
                jitter,
                mvec_scale: [-(trace.width as f32), -(trace.height as f32)],
                depth_inverted: false,
                camera_motion_included: true,
            };

            let fg_cmd_buf = dlss.evaluate_frame_generation(
                &mut encoder,
                &gpu.adapter,
                &lighting.output_color.view,
                &trace.dlss_depth.view,
                &trace.motion_vectors.view,
                fg_reset,
                fg_camera,
            )?;

            if let Some(fg_buf) = fg_cmd_buf {
                gpu.queue.submit([encoder.finish(), fg_buf]);
                encoder = gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("FG Interpolated Blit Encoder"),
                    });

                // Blit the interpolated frame to the CURRENT swapchain texture.
                // Don't present yet — submit_frame_system will render the egui
                // overlay first, then present, acquire a new texture, and blit
                // the real frame onto it (so both presented images include UI).
                if let Some(fg_output) = dlss.fg_output() {
                    let fg_bind_group = blit.create_blit_bind_group(&gpu.device, fg_output);
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("FG Interpolated Blit Pass"),
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
                        pass.set_bind_group(0, &fg_bind_group, &[]);
                        pass.draw(0..3, 0..1);
                    }

                    frame.fg_needs_real_blit = true;
                }
            }
        }
    }
    // End of DLSS scope.

    // FSR fallback — only when DLSS did not produce an upscaler buffer.
    #[cfg(feature = "fsr")]
    if upscaler_cmd_buf.is_none() {
        if let Some(mut fsr) = fsr {
            tracing::debug!(
                "render_passes: FSR active={}",
                fsr.output_texture().is_some()
            );
            if fsr.output_texture().is_some() {
                let reset = match fsr_settings.as_mut() {
                    Some(settings) if settings.enabled && settings.reset => {
                        settings.reset = false;
                        true
                    }
                    _ => false,
                };
                let jitter = temporal.current_jitter();

                fsr.render(
                    &mut encoder,
                    &gpu.queue,
                    &lighting.output_color.view,
                    &trace.dlss_depth.view,
                    &trace.motion_vectors.view,
                    reset,
                    jitter,
                    16.6, // TODO: pass actual frame delta time
                )?;
            }

            // --- FSR Frame Generation ---
            if fsr.fg_output().is_some() {
                let fg_reset = fsr_settings.as_ref().is_some_and(|s| s.reset);
                let jitter = temporal.current_jitter();
                let fwd = camera.forward();
                let right = camera.right();
                let up = right.cross(fwd);

                let fg_camera = crate::fsr::FsrFgCameraParams {
                    position: camera.position.to_array(),
                    forward: fwd.to_array(),
                    up: up.to_array(),
                    right: right.to_array(),
                    near: 0.1,
                    fov_y: camera.fov_y,
                };

                let fg_ok = fsr.evaluate_frame_generation(
                    &mut encoder,
                    &gpu.queue,
                    &trace.dlss_depth.view,
                    &trace.motion_vectors.view,
                    fg_reset,
                    jitter,
                    16.6, // TODO: pass actual frame delta time
                    fg_camera,
                )?;

                if fg_ok {
                    if let Some(fg_output) = fsr.fg_output() {
                        let fg_bind_group = blit.create_blit_bind_group(&gpu.device, fg_output);
                        {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("FSR FG Interpolated Blit Pass"),
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
                            pass.set_bind_group(0, &fg_bind_group, &[]);
                            pass.draw(0..3, 0..1);
                        }

                        frame.fg_needs_real_blit = true;
                    }
                }
            }
        }
    }

    // --- Upload tonemapping settings ----------------------------------------
    {
        let uniform = TonemappingUniform::from_settings(
            renderer_settings.tonemapping_mode,
            renderer_settings.exposure,
        );
        gpu.queue
            .write_buffer(&blit.tonemapping_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    // --- Blit real frame to swapchain --------------------------------------
    // When FG deferred the real-frame blit to submit_frame_system, the
    // swapchain already contains the interpolated blit — skip overwriting it.
    if !frame.fg_needs_real_blit {
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
