use crate::gpu_texture::GpuTexture;
use crate::pipeline_factory;
use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::shader_source;
use crate::voxel_bind_group::{bgl_storage_ro, bgl_storage_rw, bgl_storage_texture, bgl_uniform};

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
        scene: &VoxelSceneBuffers,
        lod_debug_buffer: &wgpu::Buffer,
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

    lod_debug_buffer: wgpu::Buffer,

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
            scene,
            &lod_debug_buffer,
        );

        Self {
            compute_pipeline,
            layout,
            compute_bind_group,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            lod_debug_buffer,
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
            scene,
            &self.lod_debug_buffer,
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
        self.lod_debug_buffer = create_lod_debug_buffer(device, width, height);

        self.width = width;
        self.height = height;
        self.compute_bind_group = self.layout.bind(
            device,
            &self.gbuf_color,
            &self.gbuf_normal,
            &self.gbuf_depth,
            scene,
            &self.lod_debug_buffer,
        );
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
