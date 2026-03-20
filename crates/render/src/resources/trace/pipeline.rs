use bytemuck::{Pod, Zeroable};

use crate::gpu_texture::GpuTexture;
use crate::pipeline_factory;
use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::shader_source;
use crate::voxel_bind_group::{bgl_storage_ro, bgl_storage_rw, bgl_storage_texture, bgl_uniform};

const TRACE_STATS_BUFFER_SIZE: u64 = std::mem::size_of::<TraceStatsSnapshot>() as u64;

struct TraceStatsFrame {
    readback_buffer: wgpu::Buffer,
    ready: bool,
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
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct TraceStatsSnapshot {
    pub(crate) primary_chunk_steps: u32,
    pub(crate) primary_node_steps: u32,
    pub(crate) primary_descents: u32,
    pub(crate) shadow_chunk_steps: u32,
    pub(crate) shadow_node_steps: u32,
    pub(crate) shadow_descents: u32,
    pub(crate) hit_pixels: u32,
    pub(crate) miss_pixels: u32,
    pub(crate) shadow_rays: u32,
    pub(crate) shadow_blocked: u32,
    pub(crate) lod_hits: u32,
    pub(crate) material_hits: u32,
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
            ],
        })
    }
}

pub(crate) struct TracePipeline {
    pub(crate) compute_pipeline: wgpu::ComputePipeline,
    layout: TraceLayout,
    pub(crate) compute_bind_group: wgpu::BindGroup,

    pub(crate) gbuf_color: GpuTexture,
    pub(crate) gbuf_normal: GpuTexture,
    pub(crate) gbuf_depth: GpuTexture,
    pub(crate) dlss_depth: GpuTexture,
    pub(crate) motion_vectors: GpuTexture,

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

        let layout = TraceLayout::new(device);

        let shader_source = shader_source::build_trace_shader_source();
        let compute_pipeline = pipeline_factory::create_compute_pipeline_with_layout(
            device,
            "Trace",
            &shader_source,
            layout.bgl(),
        );

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
        );

        Self {
            compute_pipeline,
            layout,
            compute_bind_group,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            dlss_depth,
            motion_vectors,
            lod_debug_buffer,
            stats_buffer,
            stats_frames: [TraceStatsFrame::new(device), TraceStatsFrame::new(device)],
            stats_frame_index: 0,
            width,
            height,
        }
    }

    pub(crate) fn rebind(&mut self, device: &wgpu::Device, scene: &VoxelSceneBuffers) {
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

        self.width = width;
        self.height = height;
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
        );
    }

    pub(crate) fn clear_stats(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.clear_buffer(&self.stats_buffer, 0, Some(TRACE_STATS_BUFFER_SIZE));
    }

    pub(crate) fn copy_stats_to_readback(&mut self, encoder: &mut wgpu::CommandEncoder) {
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
        let prev = 1 - self.stats_frame_index;
        let frame = &self.stats_frames[prev];
        if !frame.ready {
            return None;
        }

        let slice = frame.readback_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
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

    pub(crate) fn end_frame(&mut self) {
        self.stats_frame_index = 1 - self.stats_frame_index;
    }
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
