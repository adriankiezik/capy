use bytemuck::{Pod, Zeroable};

use crate::gpu_texture::GpuTexture;
use crate::pipeline_factory;
use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::shader_source::{self, TraceShaderFeatures};
use crate::voxel_bind_group::{
    bgl_sampled_texture, bgl_storage_ro, bgl_storage_rw, bgl_storage_texture, bgl_uniform,
};

const TRACE_STATS_BUFFER_SIZE: u64 = std::mem::size_of::<TraceStatsSnapshot>() as u64;
struct TraceStatsFrame {
    readback_buffer: wgpu::Buffer,
    ready: bool,
    submission: Option<wgpu::SubmissionIndex>,
}

impl TraceStatsFrame {
    fn new(device: &wgpu::Device) -> Self {
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Trace Stats Readback"),
            size: TRACE_STATS_BUFFER_SIZE,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            readback_buffer,
            ready: false,
            submission: None,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct TraceStatsSnapshot {
    pub(crate) primary_chunk_steps: u32,
    pub(crate) primary_node_steps: u32,
    pub(crate) primary_descents: u32,
    pub(crate) primary_occupied_chunks: u32,
    pub(crate) primary_empty_chunks: u32,
    pub(crate) shadow_chunk_steps: u32,
    pub(crate) shadow_node_steps: u32,
    pub(crate) shadow_descents: u32,
    pub(crate) hit_pixels: u32,
    pub(crate) miss_pixels: u32,
    pub(crate) shadow_rays: u32,
    pub(crate) shadow_blocked: u32,
    pub(crate) lod_hits: u32,
    pub(crate) material_hits: u32,
    pub(crate) grass_trace_calls: u32,
    pub(crate) grass_run_visits: u32,
    pub(crate) grass_steps: u32,
    pub(crate) grass_candidates: u32,
    pub(crate) grass_tile_rejects: u32,
    pub(crate) grass_heightmap_reads: u32,
    pub(crate) grass_column_misses: u32,
    pub(crate) grass_y_checks: u32,
    pub(crate) grass_y_rejects: u32,
    pub(crate) grass_trace_hits: u32,
    pub(crate) grass_visible_pixels: u32,
    pub(crate) grass_shadow_rays: u32,
    pub(crate) water_pixels: u32,
    pub(crate) water_top_face_pixels: u32,
    pub(crate) water_side_face_pixels: u32,
    pub(crate) water_shadow_rays: u32,
    pub(crate) water_absorb_evals: u32,
    pub(crate) water_underwater_sky: u32,
    pub(crate) water_dda_chunks_behind: u32,
    pub(crate) water_deep_no_hit: u32,
    pub(crate) water_normal_evals: u32,
    pub(crate) water_sky_evals: u32,
    pub(crate) water_normal_lod: u32,
    pub(crate) water_shadow_skipped: u32,
}

pub(crate) struct TraceLayout {
    layout: wgpu::BindGroupLayout,
}

impl TraceLayout {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Trace BGL"),
            entries: &[
                bgl_storage_texture(
                    0,
                    wgpu::TextureFormat::Rgba8Unorm,
                    wgpu::StorageTextureAccess::WriteOnly,
                ),
                bgl_uniform(1),
                bgl_uniform(2),
                bgl_storage_ro(3),
                bgl_storage_ro(4),
                bgl_storage_ro(5),
                bgl_storage_rw(6),
                bgl_storage_texture(
                    7,
                    wgpu::TextureFormat::R32Float,
                    wgpu::StorageTextureAccess::WriteOnly,
                ),
                bgl_uniform(8),
                bgl_storage_texture(
                    9,
                    wgpu::TextureFormat::Rgba8Snorm,
                    wgpu::StorageTextureAccess::WriteOnly,
                ),
                bgl_storage_texture(
                    10,
                    wgpu::TextureFormat::R32Float,
                    wgpu::StorageTextureAccess::WriteOnly,
                ),
                bgl_storage_texture(
                    11,
                    wgpu::TextureFormat::Rg32Float,
                    wgpu::StorageTextureAccess::WriteOnly,
                ),
                bgl_storage_rw(12),
                bgl_uniform(13),
                bgl_uniform(14),
                bgl_sampled_texture(15),
                bgl_sampled_texture(16),
                bgl_sampled_texture(17),
                bgl_sampled_texture(18),
                bgl_sampled_texture(19),
                bgl_sampled_texture(20),
            ],
        });
        Self { layout }
    }

    pub(crate) fn bgl(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub(crate) fn bind(
        &self,
        device: &wgpu::Device,
        gbuf_color: &GpuTexture,
        gbuf_normal: &GpuTexture,
        gbuf_depth: &GpuTexture,
        dlss_depth: &GpuTexture,
        motion_vectors: &GpuTexture,
        scene: &VoxelSceneBuffers,
        lod_debug_buffer: &wgpu::Buffer,
        stats_buffer: &wgpu::Buffer,
        near_mesh_color: &GpuTexture,
        near_mesh_normal: &GpuTexture,
        near_mesh_depth: &GpuTexture,
        near_mesh_water_normal: &GpuTexture,
        near_mesh_water_depth: &GpuTexture,
        beam_t: &GpuTexture,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Trace Bind Group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gbuf_color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: scene.camera_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: scene.streaming_info_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: scene.pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: scene.avg_pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: scene.indirection_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: lod_debug_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&gbuf_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: scene.render_settings_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&gbuf_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&dlss_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::TextureView(&motion_vectors.view),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: stats_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: scene.preview_params_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: scene.selection_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: wgpu::BindingResource::TextureView(&near_mesh_color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: wgpu::BindingResource::TextureView(&near_mesh_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 17,
                    resource: wgpu::BindingResource::TextureView(&near_mesh_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 18,
                    resource: wgpu::BindingResource::TextureView(&near_mesh_water_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 19,
                    resource: wgpu::BindingResource::TextureView(&near_mesh_water_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 20,
                    resource: wgpu::BindingResource::TextureView(&beam_t.view),
                },
            ],
        })
    }
}

/// Beam pre-pass: traces one conservative coarse ray per 8x8 pixel tile and
/// writes a start distance the full-resolution primary rays can skip to.
pub(crate) struct BeamPrepass {
    layout: wgpu::BindGroupLayout,
    pub(crate) pipeline: wgpu::ComputePipeline,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) beam_t: GpuTexture,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl BeamPrepass {
    fn create_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Beam BGL"),
            entries: &[
                bgl_uniform(0),
                bgl_uniform(1),
                bgl_storage_ro(2),
                bgl_storage_ro(3),
                bgl_storage_ro(4),
                bgl_uniform(5),
                bgl_storage_texture(
                    6,
                    wgpu::TextureFormat::R32Float,
                    wgpu::StorageTextureAccess::WriteOnly,
                ),
            ],
        })
    }

    fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> GpuTexture {
        GpuTexture::new_storage_sampled(
            device,
            "Beam Start T",
            width.div_ceil(8).max(1),
            height.div_ceil(8).max(1),
            wgpu::TextureFormat::R32Float,
        )
    }

    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        beam_t: &GpuTexture,
        scene: &VoxelSceneBuffers,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Beam Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene.camera_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: scene.streaming_info_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: scene.pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: scene.avg_pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: scene.indirection_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: scene.render_settings_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&beam_t.view),
                },
            ],
        })
    }

    fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        scene: &VoxelSceneBuffers,
        features: TraceShaderFeatures,
    ) -> Self {
        let layout = Self::create_layout(device);
        let pipeline = create_beam_compute_pipeline(device, &layout, features);
        let beam_t = Self::create_texture(device, width, height);
        let bind_group = Self::bind(device, &layout, &beam_t, scene);
        Self {
            layout,
            pipeline,
            bind_group,
            beam_t,
            width: width.div_ceil(8).max(1),
            height: height.div_ceil(8).max(1),
        }
    }

    fn resize(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        scene: &VoxelSceneBuffers,
    ) {
        self.beam_t = Self::create_texture(device, width, height);
        self.width = width.div_ceil(8).max(1);
        self.height = height.div_ceil(8).max(1);
        self.bind_group = Self::bind(device, &self.layout, &self.beam_t, scene);
    }

    fn rebind(&mut self, device: &wgpu::Device, scene: &VoxelSceneBuffers) {
        self.bind_group = Self::bind(device, &self.layout, &self.beam_t, scene);
    }

    fn update_features(&mut self, device: &wgpu::Device, features: TraceShaderFeatures) {
        self.pipeline = create_beam_compute_pipeline(device, &self.layout, features);
    }
}

pub(crate) struct TracePipeline {
    pub(crate) compute_pipeline: wgpu::ComputePipeline,
    pub(crate) features: TraceShaderFeatures,
    layout: TraceLayout,
    pub(crate) compute_bind_group: wgpu::BindGroup,
    pub(crate) beam: BeamPrepass,

    pub(crate) gbuf_color: GpuTexture,
    pub(crate) gbuf_normal: GpuTexture,
    pub(crate) gbuf_depth: GpuTexture,
    pub(crate) dlss_depth: GpuTexture,
    pub(crate) motion_vectors: GpuTexture,
    pub(crate) near_mesh_color: GpuTexture,
    pub(crate) near_mesh_normal: GpuTexture,
    pub(crate) near_mesh_depth: GpuTexture,
    pub(crate) near_mesh_depth_buffer: GpuTexture,
    pub(crate) near_mesh_water_normal: GpuTexture,
    pub(crate) near_mesh_water_depth: GpuTexture,
    pub(crate) near_mesh_water_depth_buffer: GpuTexture,

    lod_debug_buffer: wgpu::Buffer,
    stats_buffer: wgpu::Buffer,
    stats_frames: [TraceStatsFrame; 2],
    stats_frame_index: usize,

    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl TracePipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        scene: &VoxelSceneBuffers,
        features: TraceShaderFeatures,
    ) -> Self {
        let lod_debug_buffer = create_lod_debug_buffer(device, width, height);
        let stats_buffer = create_trace_stats_buffer(device);

        let gbuf_color = GpuTexture::new_storage_sampled(
            device,
            "G-Buffer Color",
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let gbuf_normal = GpuTexture::new_storage_sampled(
            device,
            "G-Buffer Normal",
            width,
            height,
            wgpu::TextureFormat::Rgba8Snorm,
        );
        let gbuf_depth = GpuTexture::new_storage_sampled(
            device,
            "G-Buffer Depth",
            width,
            height,
            wgpu::TextureFormat::R32Float,
        );
        let dlss_depth = GpuTexture::new_storage_sampled(
            device,
            "DLSS Depth",
            width,
            height,
            wgpu::TextureFormat::R32Float,
        );
        let motion_vectors = GpuTexture::new_2d(
            device,
            "Motion Vectors",
            width,
            height,
            wgpu::TextureFormat::Rg32Float,
            wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let (
            near_mesh_color,
            near_mesh_normal,
            near_mesh_depth,
            near_mesh_depth_buffer,
            near_mesh_water_normal,
            near_mesh_water_depth,
            near_mesh_water_depth_buffer,
        ) = create_near_mesh_targets(device, width, height);

        let layout = TraceLayout::new(device);

        let compute_pipeline = create_trace_compute_pipeline(device, layout.bgl(), features);
        tracing::info!(
            "Created trace shader variant: {} {features:?}",
            features.variant_label()
        );

        let beam = BeamPrepass::new(device, width, height, scene, features);

        let compute_bind_group = layout.bind(
            device,
            &gbuf_color,
            &gbuf_normal,
            &gbuf_depth,
            &dlss_depth,
            &motion_vectors,
            scene,
            &lod_debug_buffer,
            &stats_buffer,
            &near_mesh_color,
            &near_mesh_normal,
            &near_mesh_depth,
            &near_mesh_water_normal,
            &near_mesh_water_depth,
            &beam.beam_t,
        );

        Self {
            compute_pipeline,
            features,
            layout,
            compute_bind_group,
            beam,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            dlss_depth,
            motion_vectors,
            near_mesh_color,
            near_mesh_normal,
            near_mesh_depth,
            near_mesh_depth_buffer,
            near_mesh_water_normal,
            near_mesh_water_depth,
            near_mesh_water_depth_buffer,
            lod_debug_buffer,
            stats_buffer,
            stats_frames: [TraceStatsFrame::new(device), TraceStatsFrame::new(device)],
            stats_frame_index: 0,
            width,
            height,
        }
    }

    pub(crate) fn rebind(&mut self, device: &wgpu::Device, scene: &VoxelSceneBuffers) {
        self.beam.rebind(device, scene);
        self.compute_bind_group = self.layout.bind(
            device,
            &self.gbuf_color,
            &self.gbuf_normal,
            &self.gbuf_depth,
            &self.dlss_depth,
            &self.motion_vectors,
            scene,
            &self.lod_debug_buffer,
            &self.stats_buffer,
            &self.near_mesh_color,
            &self.near_mesh_normal,
            &self.near_mesh_depth,
            &self.near_mesh_water_normal,
            &self.near_mesh_water_depth,
            &self.beam.beam_t,
        );
    }

    pub(crate) fn update_features(&mut self, device: &wgpu::Device, features: TraceShaderFeatures) {
        if self.features == features {
            return;
        }

        self.compute_pipeline = create_trace_compute_pipeline(device, self.layout.bgl(), features);
        self.beam.update_features(device, features);
        self.features = features;
        tracing::info!(
            "Rebuilt trace shader variant: {} {features:?}",
            features.variant_label()
        );
    }

    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        scene: &VoxelSceneBuffers,
    ) {
        self.gbuf_color = GpuTexture::new_storage_sampled(
            device,
            "G-Buffer Color",
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        self.gbuf_normal = GpuTexture::new_storage_sampled(
            device,
            "G-Buffer Normal",
            width,
            height,
            wgpu::TextureFormat::Rgba8Snorm,
        );
        self.gbuf_depth = GpuTexture::new_storage_sampled(
            device,
            "G-Buffer Depth",
            width,
            height,
            wgpu::TextureFormat::R32Float,
        );
        self.dlss_depth = GpuTexture::new_storage_sampled(
            device,
            "DLSS Depth",
            width,
            height,
            wgpu::TextureFormat::R32Float,
        );
        self.motion_vectors = GpuTexture::new_2d(
            device,
            "Motion Vectors",
            width,
            height,
            wgpu::TextureFormat::Rg32Float,
            wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        self.lod_debug_buffer = create_lod_debug_buffer(device, width, height);
        (
            self.near_mesh_color,
            self.near_mesh_normal,
            self.near_mesh_depth,
            self.near_mesh_depth_buffer,
            self.near_mesh_water_normal,
            self.near_mesh_water_depth,
            self.near_mesh_water_depth_buffer,
        ) = create_near_mesh_targets(device, width, height);

        self.width = width;
        self.height = height;
        self.beam.resize(device, width, height, scene);
        self.compute_bind_group = self.layout.bind(
            device,
            &self.gbuf_color,
            &self.gbuf_normal,
            &self.gbuf_depth,
            &self.dlss_depth,
            &self.motion_vectors,
            scene,
            &self.lod_debug_buffer,
            &self.stats_buffer,
            &self.near_mesh_color,
            &self.near_mesh_normal,
            &self.near_mesh_depth,
            &self.near_mesh_water_normal,
            &self.near_mesh_water_depth,
            &self.beam.beam_t,
        );
    }

    pub(crate) fn clear_stats(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.features.trace_stats {
            return;
        }
        encoder.clear_buffer(&self.stats_buffer, 0, Some(TRACE_STATS_BUFFER_SIZE));
    }

    pub(crate) fn copy_stats_to_readback(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.features.trace_stats {
            return;
        }
        let frame = &mut self.stats_frames[self.stats_frame_index];
        encoder.copy_buffer_to_buffer(
            &self.stats_buffer,
            0,
            &frame.readback_buffer,
            0,
            TRACE_STATS_BUFFER_SIZE,
        );
        frame.ready = true;
    }

    pub(crate) fn read_back_stats(&self, device: &wgpu::Device) -> Option<TraceStatsSnapshot> {
        if !self.features.trace_stats {
            return None;
        }
        let prev = 1 - self.stats_frame_index;
        let frame = &self.stats_frames[prev];
        if !frame.ready {
            return None;
        }

        let slice = frame.readback_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        // Wait only for the previous frame's submission (already complete) —
        // waiting on the latest would serialize CPU and GPU.
        device
            .poll(wgpu::PollType::Wait {
                submission_index: frame.submission.clone(),
                timeout: None,
            })
            .ok();

        let snapshot = {
            let data = slice.get_mapped_range();
            *bytemuck::from_bytes::<TraceStatsSnapshot>(&data[..TRACE_STATS_BUFFER_SIZE as usize])
        };
        frame.readback_buffer.unmap();
        Some(snapshot)
    }

    /// Record the submission that contains this frame's stats readback copy.
    pub(crate) fn set_stats_submission(&mut self, submission: wgpu::SubmissionIndex) {
        if !self.features.trace_stats {
            return;
        }
        self.stats_frames[self.stats_frame_index].submission = Some(submission);
    }

    pub(crate) fn end_frame(&mut self) {
        self.stats_frame_index = 1 - self.stats_frame_index;
    }
}

fn create_near_mesh_targets(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (
    GpuTexture,
    GpuTexture,
    GpuTexture,
    GpuTexture,
    GpuTexture,
    GpuTexture,
    GpuTexture,
) {
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    (
        GpuTexture::new_2d(
            device,
            "Near Mesh Color",
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            usage,
        ),
        GpuTexture::new_2d(
            device,
            "Near Mesh Normal",
            width,
            height,
            wgpu::TextureFormat::Rgba16Float,
            usage,
        ),
        GpuTexture::new_2d(
            device,
            "Near Mesh Ray Depth",
            width,
            height,
            wgpu::TextureFormat::R32Float,
            usage,
        ),
        GpuTexture::new_2d(
            device,
            "Near Mesh Depth Buffer",
            width,
            height,
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        ),
        GpuTexture::new_2d(
            device,
            "Near Mesh Water Normal",
            width,
            height,
            wgpu::TextureFormat::Rgba16Float,
            usage,
        ),
        GpuTexture::new_2d(
            device,
            "Near Mesh Water Ray Depth",
            width,
            height,
            wgpu::TextureFormat::R32Float,
            usage,
        ),
        GpuTexture::new_2d(
            device,
            "Near Mesh Water Depth Buffer",
            width,
            height,
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        ),
    )
}

fn create_trace_compute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    features: TraceShaderFeatures,
) -> wgpu::ComputePipeline {
    let shader_source = shader_source::build_trace_shader_source(features);
    let label = if features.is_primary_only() {
        "Trace PrimaryOnly"
    } else {
        "Trace"
    };
    pipeline_factory::create_compute_pipeline_with_layout(device, label, &shader_source, layout)
}

fn create_beam_compute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    features: TraceShaderFeatures,
) -> wgpu::ComputePipeline {
    let shader_source = shader_source::build_beam_shader_source(features);
    pipeline_factory::create_compute_pipeline_with_layout(
        device,
        "Beam Prepass",
        &shader_source,
        layout,
    )
}

fn create_lod_debug_buffer(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Buffer {
    let lod_debug_size = (width * height) as usize;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("LOD Debug"),
        size: (lod_debug_size * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    })
}

fn create_trace_stats_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Trace Stats"),
        size: TRACE_STATS_BUFFER_SIZE,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}
