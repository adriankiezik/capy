use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::shader_source;

pub(crate) struct TracePipeline {
    pub(crate) compute_pipeline: wgpu::ComputePipeline,
    compute_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) compute_bind_group: wgpu::BindGroup,

    pub(crate) gbuf_color: wgpu::Texture,
    pub(crate) gbuf_normal: wgpu::Texture,
    pub(crate) gbuf_depth: wgpu::Texture,

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
        let lod_debug_size = (width * height) as usize;
        let lod_debug_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Debug"),
            size: (lod_debug_size * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let gbuf_color = create_gbuf_color(device, width, height);
        let gbuf_normal = create_gbuf_normal(device, width, height);
        let gbuf_depth = create_gbuf_depth(device, width, height);

        let shader_source = shader_source::build_trace_shader_source();
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Trace Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Trace BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::R32Float,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 9,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Snorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Trace Pipeline Layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                ..Default::default()
            });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Trace Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let compute_bind_group = Self::build_bind_group(
            device,
            &compute_bind_group_layout,
            &gbuf_color,
            &gbuf_normal,
            &gbuf_depth,
            scene,
            &lod_debug_buffer,
        );

        Self {
            compute_pipeline,
            compute_bind_group_layout,
            compute_bind_group,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            lod_debug_buffer,
            width,
            height,
        }
    }

    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        scene: &VoxelSceneBuffers,
    ) {
        self.gbuf_color = create_gbuf_color(device, width, height);
        self.gbuf_normal = create_gbuf_normal(device, width, height);
        self.gbuf_depth = create_gbuf_depth(device, width, height);

        let lod_debug_size = (width * height) as usize;
        self.lod_debug_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Debug"),
            size: (lod_debug_size * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        self.width = width;
        self.height = height;
        self.rebuild_bind_group(device, scene);
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device, scene: &VoxelSceneBuffers) {
        self.compute_bind_group = Self::build_bind_group(
            device,
            &self.compute_bind_group_layout,
            &self.gbuf_color,
            &self.gbuf_normal,
            &self.gbuf_depth,
            scene,
            &self.lod_debug_buffer,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn build_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        gbuf_color: &wgpu::Texture,
        gbuf_normal: &wgpu::Texture,
        gbuf_depth: &wgpu::Texture,
        scene: &VoxelSceneBuffers,
        lod_debug_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let color_view = gbuf_color.create_view(&wgpu::TextureViewDescriptor::default());
        let normal_view = gbuf_normal.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = gbuf_depth.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Trace Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: scene.camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: scene.streaming_info_buffer.as_entire_binding(),
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
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: scene.render_settings_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&normal_view),
                },
            ],
        })
    }
}

fn create_gbuf_color(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("G-Buffer Color"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_gbuf_normal(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("G-Buffer Normal"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Snorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_gbuf_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("G-Buffer Depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
