use bytemuck::{Pod, Zeroable};

use crate::gpu_texture::GpuTexture;
use crate::pipeline_factory;
use crate::resources::RendererSettings;
use crate::shader_source;
use crate::uniform_buffer::UniformBuffer;
use crate::voxel_bind_group::{bgl_sampled_texture, bgl_storage_texture, bgl_uniform};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GtaoParamsUniform {
    pub(crate) ao_radius: f32,
    pub(crate) ao_intensity: f32,
    pub(crate) ao_samples: u32,
    pub(crate) ao_steps: u32,
}

const _: () = assert!(std::mem::size_of::<GtaoParamsUniform>() == 16);

struct GtaoLayout {
    layout: wgpu::BindGroupLayout,
}

impl GtaoLayout {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GTAO BGL"),
            entries: &[
                bgl_sampled_texture(0),
                bgl_sampled_texture(1),
                bgl_storage_texture(
                    2,
                    wgpu::TextureFormat::R32Float,
                    wgpu::StorageTextureAccess::WriteOnly,
                ),
                bgl_uniform(3),
                bgl_uniform(4),
            ],
        });
        Self { layout }
    }

    fn bgl(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    fn bind(
        &self,
        device: &wgpu::Device,
        gbuf_depth: &GpuTexture,
        gbuf_normal: &GpuTexture,
        ao_output: &GpuTexture,
        camera_buffer: &wgpu::Buffer,
        gtao_params_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GTAO Bind Group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gbuf_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gbuf_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&ao_output.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: gtao_params_buffer.as_entire_binding(),
                },
            ],
        })
    }
}

pub(crate) struct GtaoPipeline {
    pub(crate) compute_pipeline: wgpu::ComputePipeline,
    layout: GtaoLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) ao_output: GpuTexture,
    gtao_params_buffer: UniformBuffer<GtaoParamsUniform>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl GtaoPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        gbuf_depth: &GpuTexture,
        gbuf_normal: &GpuTexture,
        camera_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
        settings: &RendererSettings,
    ) -> Self {
        let ao_output = GpuTexture::new_storage_sampled(
            device,
            "GTAO Output",
            width,
            height,
            wgpu::TextureFormat::R32Float,
        );

        let params = params_from_settings(settings);
        let gtao_params_buffer = UniformBuffer::new(device, "GTAO Params", &params);

        let layout = GtaoLayout::new(device);

        let shader_source = shader_source::build_gtao_shader_source();
        let compute_pipeline = pipeline_factory::create_compute_pipeline_with_layout(
            device,
            "GTAO",
            &shader_source,
            layout.bgl(),
        );

        let bind_group = layout.bind(
            device,
            gbuf_depth,
            gbuf_normal,
            &ao_output,
            camera_buffer,
            gtao_params_buffer.buffer(),
        );

        Self {
            compute_pipeline,
            layout,
            bind_group,
            ao_output,
            gtao_params_buffer,
            width,
            height,
        }
    }

    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        gbuf_depth: &GpuTexture,
        gbuf_normal: &GpuTexture,
        camera_buffer: &wgpu::Buffer,
        size: [u32; 2],
    ) {
        let [width, height] = size;
        self.ao_output = GpuTexture::new_storage_sampled(
            device,
            "GTAO Output",
            width,
            height,
            wgpu::TextureFormat::R32Float,
        );
        self.width = width;
        self.height = height;
        self.bind_group = self.layout.bind(
            device,
            gbuf_depth,
            gbuf_normal,
            &self.ao_output,
            camera_buffer,
            self.gtao_params_buffer.buffer(),
        );
    }

    pub(crate) fn update_params(&self, queue: &wgpu::Queue, settings: &RendererSettings) {
        self.gtao_params_buffer
            .write(queue, &params_from_settings(settings));
    }
}

fn params_from_settings(settings: &RendererSettings) -> GtaoParamsUniform {
    GtaoParamsUniform {
        ao_radius: settings.ao_radius,
        ao_intensity: settings.ao_intensity,
        ao_samples: settings.ao_samples,
        ao_steps: settings.ao_steps,
    }
}
