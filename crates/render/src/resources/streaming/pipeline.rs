use wgpu::util::DeviceExt;

use capy_core::{Camera, VoxelMeshData};

use crate::camera::CameraUniform;
use crate::settings::{RendererSettings, StreamingInfoUniform};
use crate::shader_source;

pub(crate) struct StreamingPipeline {
    pub(crate) compute_pipeline: wgpu::ComputePipeline,
    compute_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) compute_bind_group: wgpu::BindGroup,

    pub(crate) storage_texture: wgpu::Texture,
    pub(crate) depth_texture: wgpu::Texture,

    pub(crate) camera_buffer: wgpu::Buffer,
    render_settings_buffer: wgpu::Buffer,
    pub(crate) lod_bias: f32,

    streaming_info_buffer: wgpu::Buffer,
    pool_buffer: wgpu::Buffer,
    avg_pool_buffer: wgpu::Buffer,
    indirection_buffer: wgpu::Buffer,
    lod_debug_buffer: wgpu::Buffer,
}

impl StreamingPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        mesh: &VoxelMeshData,
        camera: &Camera,
    ) -> Self {
        // Camera uniform
        let lod_bias = 1.0;
        let camera_uniform = CameraUniform::from_camera(camera, width, height, lod_bias);
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Render settings
        let settings = RendererSettings::with_palette(mesh.material_palette);
        let render_settings_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Render Settings Uniform"),
            contents: bytemuck::bytes_of(&settings.to_uniform()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Chunk data buffers
        let pool_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Chunk Pool"),
            contents: bytemuck::cast_slice(&mesh.dag_buffer),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let avg_pool_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Avg Color Pool"),
            contents: bytemuck::cast_slice(&mesh.avg_color_buffer),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Single-chunk indirection: [world_size, root_offset, depth, pool_offset=0]
        let indirection_data: [u32; 4] = [
            mesh.world_size,
            mesh.root_offset,
            mesh.depth,
            0, // pool_offset for single chunk
        ];
        let indirection_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Indirection"),
            contents: bytemuck::cast_slice(&indirection_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Streaming info for single chunk at grid (0,0,0) with dim=1
        let streaming_info = StreamingInfoUniform {
            grid_min_x: 0,
            grid_min_y: 0,
            grid_min_z: 0,
            grid_dim: 1,
            chunk_size: mesh.chunk_size,
            pool_slot_count: 1,
            _pad0: 0,
            _pad1: 0,
        };
        let streaming_info_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Streaming Info"),
            contents: bytemuck::bytes_of(&streaming_info),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // LOD debug buffer
        let lod_debug_size = (width * height) as usize;
        let lod_debug_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Debug"),
            size: (lod_debug_size * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        // Storage textures
        let storage_texture = create_storage_texture(device, width, height);
        let depth_texture = create_depth_texture(device, width, height);

        // Pipeline
        let shader_source = shader_source::build_streaming_shader_source();
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Streaming Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Streaming BGL"),
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
                ],
            });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Streaming Pipeline Layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                ..Default::default()
            });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Streaming Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let compute_bind_group = Self::build_bind_group(
            device,
            &compute_bind_group_layout,
            &storage_texture,
            &depth_texture,
            &camera_buffer,
            &render_settings_buffer,
            &streaming_info_buffer,
            &pool_buffer,
            &avg_pool_buffer,
            &indirection_buffer,
            &lod_debug_buffer,
        );

        Self {
            compute_pipeline,
            compute_bind_group_layout,
            compute_bind_group,
            storage_texture,
            depth_texture,
            camera_buffer,
            render_settings_buffer,
            lod_bias,
            streaming_info_buffer,
            pool_buffer,
            avg_pool_buffer,
            indirection_buffer,
            lod_debug_buffer,
        }
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.storage_texture = create_storage_texture(device, width, height);
        self.depth_texture = create_depth_texture(device, width, height);

        let lod_debug_size = (width * height) as usize;
        self.lod_debug_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Debug"),
            size: (lod_debug_size * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        self.rebuild_bind_group(device);
    }

    pub(crate) fn upload_camera(
        &self,
        queue: &wgpu::Queue,
        camera: &Camera,
        width: u32,
        height: u32,
    ) {
        let uniform = CameraUniform::from_camera(camera, width, height, self.lod_bias);
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        self.compute_bind_group = Self::build_bind_group(
            device,
            &self.compute_bind_group_layout,
            &self.storage_texture,
            &self.depth_texture,
            &self.camera_buffer,
            &self.render_settings_buffer,
            &self.streaming_info_buffer,
            &self.pool_buffer,
            &self.avg_pool_buffer,
            &self.indirection_buffer,
            &self.lod_debug_buffer,
        );
    }

    fn build_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        storage_texture: &wgpu::Texture,
        depth_texture: &wgpu::Texture,
        camera_buffer: &wgpu::Buffer,
        render_settings_buffer: &wgpu::Buffer,
        streaming_info_buffer: &wgpu::Buffer,
        pool_buffer: &wgpu::Buffer,
        avg_pool_buffer: &wgpu::Buffer,
        indirection_buffer: &wgpu::Buffer,
        lod_debug_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let view = storage_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Streaming Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: streaming_info_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: avg_pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: indirection_buffer.as_entire_binding(),
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
                    resource: render_settings_buffer.as_entire_binding(),
                },
            ],
        })
    }
}

fn create_storage_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Storage Texture"),
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

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    })
}
