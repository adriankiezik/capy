use crate::gpu_texture::GpuTexture;
use crate::shader_source;
use crate::voxel_bind_group::bgl_sampler_filtering;

pub(crate) struct BlitLayout {
    layout: wgpu::BindGroupLayout,
}

impl BlitLayout {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blit BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                bgl_sampler_filtering(1),
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
        storage_texture: &GpuTexture,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blit Bind Group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&storage_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}

pub(crate) struct BlitPipeline {
    pub(crate) blit_pipeline: wgpu::RenderPipeline,
    layout: BlitLayout,
    pub(crate) blit_bind_group: wgpu::BindGroup,
    blit_sampler: wgpu::Sampler,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) source_width: u32,
    pub(crate) source_height: u32,
    /// True when the blit source is an upscaler output (DLSS or FSR) at output resolution.
    pub(crate) source_is_upscaled: bool,
    /// Distinguishes DLSS SR from DLSS RR so we rebind when switching between them.
    pub(crate) source_is_rr: bool,
    /// Tracks upscaler output texture recreation (quality changes, SR↔RR switches).
    pub(crate) upscaler_generation: u32,
}

impl BlitPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        storage_texture: &GpuTexture,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        source_is_upscaled: bool,
    ) -> Self {
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blit Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source::SHADER_BLIT.into()),
        });

        let layout = BlitLayout::new(device);

        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Blit Pipeline Layout"),
            bind_group_layouts: &[layout.bgl()],
            ..Default::default()
        });

        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Blit Pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Blit Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let blit_bind_group = layout.bind(device, storage_texture, &blit_sampler);

        Self {
            blit_pipeline,
            layout,
            blit_bind_group,
            blit_sampler,
            width,
            height,
            source_width: width,
            source_height: height,
            source_is_upscaled,
            source_is_rr: false,
            upscaler_generation: 0,
        }
    }

    pub(crate) fn rebuild_bind_group(
        &mut self,
        device: &wgpu::Device,
        storage_texture: &GpuTexture,
    ) {
        self.blit_bind_group = self
            .layout
            .bind(device, storage_texture, &self.blit_sampler);
    }

    /// Create a one-off bind group for blitting a different source texture
    /// (e.g. Frame Generation interpolated output).
    #[cfg_attr(not(feature = "dlss"), allow(dead_code))]
    pub(crate) fn create_blit_bind_group(
        &self,
        device: &wgpu::Device,
        source: &GpuTexture,
    ) -> wgpu::BindGroup {
        self.layout.bind(device, source, &self.blit_sampler)
    }
}
