use crate::shader_source;

pub(crate) struct LightingPipeline {
    pub(crate) compute_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) output_color: wgpu::Texture,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl LightingPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        gbuf_color: &wgpu::Texture,
        gbuf_normal: &wgpu::Texture,
        gbuf_depth: &wgpu::Texture,
        render_settings_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> Self {
        let output_color = create_output_color(device, width, height);

        let shader_source = shader_source::build_lighting_shader_source();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Lighting Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Lighting BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Lighting Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Lighting Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let bind_group = Self::build_bind_group(
            device,
            &bind_group_layout,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            &output_color,
            render_settings_buffer,
        );

        Self {
            compute_pipeline,
            bind_group_layout,
            bind_group,
            output_color,
            width,
            height,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        gbuf_color: &wgpu::Texture,
        gbuf_normal: &wgpu::Texture,
        gbuf_depth: &wgpu::Texture,
        render_settings_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) {
        self.output_color = create_output_color(device, width, height);
        self.width = width;
        self.height = height;
        self.bind_group = Self::build_bind_group(
            device,
            &self.bind_group_layout,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            &self.output_color,
            render_settings_buffer,
        );
    }

    fn build_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        gbuf_color: &wgpu::Texture,
        gbuf_normal: &wgpu::Texture,
        gbuf_depth: &wgpu::Texture,
        output_color: &wgpu::Texture,
        render_settings_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let color_view = gbuf_color.create_view(&wgpu::TextureViewDescriptor::default());
        let normal_view = gbuf_normal.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = gbuf_depth.create_view(&wgpu::TextureViewDescriptor::default());
        let output_view = output_color.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: render_settings_buffer.as_entire_binding(),
                },
            ],
        })
    }
}

fn create_output_color(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Lighting Output Color"),
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
