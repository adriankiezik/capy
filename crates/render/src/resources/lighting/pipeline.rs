use crate::gpu_texture::GpuTexture;
use crate::pipeline_factory;
use crate::shader_source;
use crate::voxel_bind_group::{bgl_sampled_texture, bgl_storage_texture, bgl_uniform};

pub(crate) struct LightingLayout {
    layout: wgpu::BindGroupLayout,
}

impl LightingLayout {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Lighting BGL"),
            entries: &[
                bgl_sampled_texture(0),
                bgl_sampled_texture(1),
                bgl_sampled_texture(2),
                bgl_storage_texture(
                    3,
                    wgpu::TextureFormat::Rgba8Unorm,
                    wgpu::StorageTextureAccess::WriteOnly,
                ),
                bgl_uniform(4),
                bgl_sampled_texture(5),
                bgl_uniform(6),
            ],
        });
        Self { layout }
    }

    pub(crate) fn bgl(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bind(
        &self,
        device: &wgpu::Device,
        gbuf_color: &GpuTexture,
        gbuf_normal: &GpuTexture,
        gbuf_depth: &GpuTexture,
        output_color: &GpuTexture,
        render_settings_buffer: &wgpu::Buffer,
        ao_texture: &GpuTexture,
        camera_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Bind Group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gbuf_color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gbuf_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&gbuf_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&output_color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: render_settings_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&ao_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: camera_buffer.as_entire_binding(),
                },
            ],
        })
    }
}

pub(crate) struct LightingPipeline {
    pub(crate) compute_pipeline: wgpu::ComputePipeline,
    layout: LightingLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) output_color: GpuTexture,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) ao_source_is_rtao: bool,
}

impl LightingPipeline {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        device: &wgpu::Device,
        gbuf_color: &GpuTexture,
        gbuf_normal: &GpuTexture,
        gbuf_depth: &GpuTexture,
        render_settings_buffer: &wgpu::Buffer,
        ao_texture: &GpuTexture,
        camera_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> Self {
        let output_color = GpuTexture::new_storage_sampled(
            device,
            "Lighting Output Color",
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
        );

        let layout = LightingLayout::new(device);

        let shader_source = shader_source::build_lighting_shader_source();
        let compute_pipeline = pipeline_factory::create_compute_pipeline_with_layout(
            device,
            "Lighting",
            &shader_source,
            layout.bgl(),
        );

        let bind_group = layout.bind(
            device,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            &output_color,
            render_settings_buffer,
            ao_texture,
            camera_buffer,
        );

        Self {
            compute_pipeline,
            layout,
            bind_group,
            output_color,
            width,
            height,
            ao_source_is_rtao: false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rebind_ao(
        &mut self,
        device: &wgpu::Device,
        gbuf_color: &GpuTexture,
        gbuf_normal: &GpuTexture,
        gbuf_depth: &GpuTexture,
        render_settings_buffer: &wgpu::Buffer,
        ao_texture: &GpuTexture,
        camera_buffer: &wgpu::Buffer,
    ) {
        self.bind_group = self.layout.bind(
            device,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            &self.output_color,
            render_settings_buffer,
            ao_texture,
            camera_buffer,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        gbuf_color: &GpuTexture,
        gbuf_normal: &GpuTexture,
        gbuf_depth: &GpuTexture,
        render_settings_buffer: &wgpu::Buffer,
        ao_texture: &GpuTexture,
        camera_buffer: &wgpu::Buffer,
        size: [u32; 2],
    ) {
        let [width, height] = size;
        self.output_color = GpuTexture::new_storage_sampled(
            device,
            "Lighting Output Color",
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        self.width = width;
        self.height = height;
        self.bind_group = self.layout.bind(
            device,
            gbuf_color,
            gbuf_normal,
            gbuf_depth,
            &self.output_color,
            render_settings_buffer,
            ao_texture,
            camera_buffer,
        );
    }
}
