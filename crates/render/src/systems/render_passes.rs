use bevy_ecs::error::BevyError;
use bevy_ecs::system::{NonSend, NonSendMut, Res, ResMut};

use crate::resources::TemporalCameraState;
use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::resources::{
    BlitPipeline, FrameInProgress, GpuContext, GpuProfiler, GtaoPipeline, LightingPipeline,
    NearMeshPipeline,
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
    near_mesh: Option<NonSend<NearMeshPipeline>>,
    scene: Option<NonSend<VoxelSceneBuffers>>,
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

    if let (Some(near_mesh), Some(scene)) = (near_mesh, scene)
        && renderer_settings.hybrid_near_radius > 0.0
        && (scene.near_mesh_index_count > 0 || scene.near_mesh_water_index_count > 0)
        && (scene
            .near_mesh_chunks
            .iter()
            .chain(scene.near_mesh_water_chunks.iter())
            .any(|chunk| {
                near_mesh_chunk_is_active(
                    chunk.coord,
                    scene.chunk_size_xz,
                    camera.position,
                    renderer_settings.hybrid_near_radius,
                )
            })
            || ((scene.canonical_index_count > 0 || scene.canonical_water_index_count > 0)
                && scene.canonical_chunks.iter().any(|&coord| {
                    near_mesh_chunk_is_active(
                        coord,
                        scene.chunk_size_xz,
                        camera.position,
                        renderer_settings.hybrid_near_radius,
                    )
                })))
    {
        let ts = gpu_profiler.pass_indices("near-mesh");
        let opaque_ts_writes = ts.map(|(b, _)| wgpu::RenderPassTimestampWrites {
            query_set: gpu_profiler.query_set(),
            beginning_of_pass_write_index: Some(b),
            end_of_pass_write_index: None,
        });
        let water_ts_writes = ts.map(|(_, e)| wgpu::RenderPassTimestampWrites {
            query_set: gpu_profiler.query_set(),
            beginning_of_pass_write_index: None,
            end_of_pass_write_index: Some(e),
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Near Mesh Pass"),
            color_attachments: &[
                Some(clear_color_attachment(&trace.near_mesh_color.view)),
                Some(clear_color_attachment(&trace.near_mesh_normal.view)),
                Some(clear_color_attachment(&trace.near_mesh_depth.view)),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &trace.near_mesh_depth_buffer.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: opaque_ts_writes,
            ..Default::default()
        });
        draw_near_mesh(
            &mut pass,
            &near_mesh.opaque_pipeline,
            &near_mesh.bind_group,
            &scene,
            &scene.near_mesh_index_buffer,
            &scene.near_mesh_chunks,
            scene.canonical_index_start,
            scene.canonical_index_count,
            camera.position,
            renderer_settings.hybrid_near_radius,
        );
        drop(pass);

        let mut water_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Near Water Mesh Pass"),
            color_attachments: &[
                Some(clear_color_attachment(&trace.near_mesh_water_normal.view)),
                Some(clear_color_attachment(&trace.near_mesh_water_depth.view)),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &trace.near_mesh_water_depth_buffer.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: water_ts_writes,
            ..Default::default()
        });
        if renderer_settings.water_enabled {
            draw_near_mesh(
                &mut water_pass,
                &near_mesh.water_pipeline,
                &near_mesh.bind_group,
                &scene,
                &scene.near_mesh_water_index_buffer,
                &scene.near_mesh_water_chunks,
                scene.canonical_water_index_start,
                scene.canonical_water_index_count,
                camera.position,
                renderer_settings.hybrid_near_radius,
            );
        }
    } else {
        let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Near Mesh Depth Clear"),
            color_attachments: &[
                Some(clear_color_attachment(&trace.near_mesh_depth.view)),
                Some(clear_color_attachment(&trace.near_mesh_water_depth.view)),
            ],
            ..Default::default()
        });
    }

    if trace.features.beam {
        let ts = gpu_profiler.pass_indices("beam");
        let ts_writes = ts.map(|(b, e)| wgpu::ComputePassTimestampWrites {
            query_set: gpu_profiler.query_set(),
            beginning_of_pass_write_index: Some(b),
            end_of_pass_write_index: Some(e),
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Beam Prepass"),
            timestamp_writes: ts_writes,
        });
        pass.set_pipeline(&trace.beam.pipeline);
        pass.set_bind_group(0, &trace.beam.bind_group, &[]);
        pass.dispatch_workgroups(
            trace.beam.width.div_ceil(8),
            trace.beam.height.div_ceil(8),
            1,
        );
    }

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
        // Trace entry uses @workgroup_size(8, 4): one 32-lane simdgroup per group.
        pass.dispatch_workgroups(trace.width.div_ceil(8), trace.height.div_ceil(4), 1);
    }
    trace.copy_stats_to_readback(&mut encoder);

    if renderer_settings.ao_intensity > 0.0
        && renderer_settings.ao_samples > 0
        && renderer_settings.ao_steps > 0
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

fn clear_color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        depth_slice: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    }
}

fn draw_near_mesh<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    pipeline: &'pass wgpu::RenderPipeline,
    bind_group: &'pass wgpu::BindGroup,
    scene: &'pass VoxelSceneBuffers,
    index_buffer: &'pass wgpu::Buffer,
    chunks: &'pass [capy_core::NearVoxelMeshChunk],
    canonical_index_start: u32,
    canonical_index_count: u32,
    camera_position: glam::Vec3,
    radius: f32,
) {
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.set_vertex_buffer(0, scene.near_mesh_vertex_buffer.slice(..));
    pass.set_vertex_buffer(1, scene.near_mesh_instance_buffer.slice(..));
    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    for chunk in chunks {
        if near_mesh_chunk_is_active(chunk.coord, scene.chunk_size_xz, camera_position, radius) {
            pass.draw_indexed(
                chunk.index_start..chunk.index_start + chunk.index_count,
                0,
                0..1,
            );
        }
    }
    if canonical_index_count > 0 {
        let index_end = canonical_index_start + canonical_index_count;
        for (instance_index, &coord) in scene.canonical_chunks.iter().enumerate() {
            if near_mesh_chunk_is_active(coord, scene.chunk_size_xz, camera_position, radius) {
                let instance = instance_index as u32 + 1;
                pass.draw_indexed(canonical_index_start..index_end, 0, instance..instance + 1);
            }
        }
    }
}

fn near_mesh_chunk_is_active(
    coord: [i32; 3],
    chunk_size_xz: u32,
    camera_position: glam::Vec3,
    radius: f32,
) -> bool {
    let chunk_size = chunk_size_xz as f32;
    let center_x = (coord[0] as f32 + 0.5) * chunk_size;
    let center_z = (coord[2] as f32 + 0.5) * chunk_size;
    let dx = center_x - camera_position.x;
    let dz = center_z - camera_position.z;
    dx * dx + dz * dz <= radius * radius
}
