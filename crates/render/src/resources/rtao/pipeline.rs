use bytemuck::{Pod, Zeroable};

use crate::gpu_texture::GpuTexture;
use crate::pipeline_factory;
use crate::resources::RendererSettings;
use crate::resources::SharedVoxelBuffers;
use crate::shader_source;
use crate::uniform_buffer::UniformBuffer;
use crate::voxel_bind_group::{
    bgl_sampled_texture, bgl_storage_ro, bgl_storage_texture, bgl_uniform,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct RtaoParamsUniform {
    pub(crate) ao_radius: f32,
    pub(crate) ao_intensity: f32,
    pub(crate) ao_rays: u32,
    pub(crate) frame_index: u32,
}

const _: () = assert!(std::mem::size_of::<RtaoParamsUniform>() == 16);

struct RtaoLayout {
    layout: wgpu::BindGroupLayout,
}

impl RtaoLayout {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RTAO BGL"),
            entries: &[
                // Voxel scene bindings 0-5
                bgl_uniform(0),    // camera
                bgl_uniform(1),    // streaming
                bgl_storage_ro(2), // chunk_pool
                bgl_storage_ro(3), // chunk_avg_pool
                bgl_storage_ro(4), // indirection
                bgl_uniform(5),    // render_settings
                // RTAO-specific bindings 6-9
                bgl_sampled_texture(6), // gbuf_depth
                bgl_sampled_texture(7), // gbuf_normal
                bgl_storage_texture(
                    8,
                    wgpu::TextureFormat::R32Float,
                    wgpu::StorageTextureAccess::WriteOnly,
                ), // ao_output
                bgl_uniform(9),         // rtao_params
            ],
        });
        Self { layout }
    }

    fn bgl(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    #[allow(clippy::too_many_arguments)]
    fn bind(
        &self,
        device: &wgpu::Device,
        voxels: &SharedVoxelBuffers,
        gbuf_depth: &GpuTexture,
        gbuf_normal: &GpuTexture,
        ao_output: &GpuTexture,
        rtao_params_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RTAO Bind Group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: voxels.camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: voxels.streaming_info_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: voxels.pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: voxels.avg_pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: voxels.indirection_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: voxels.render_settings_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&gbuf_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&gbuf_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&ao_output.view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: rtao_params_buffer.as_entire_binding(),
                },
            ],
        })
    }
}

pub(crate) struct RtaoPipeline {
    pub(crate) compute_pipeline: wgpu::ComputePipeline,
    layout: RtaoLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) ao_output: GpuTexture,
    rtao_params_buffer: UniformBuffer<RtaoParamsUniform>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl RtaoPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        gbuf_depth: &GpuTexture,
        gbuf_normal: &GpuTexture,
        voxels: &SharedVoxelBuffers,
        width: u32,
        height: u32,
        settings: &RendererSettings,
    ) -> Self {
        let ao_output = GpuTexture::new_storage_sampled(
            device,
            "RTAO Output",
            width,
            height,
            wgpu::TextureFormat::R32Float,
        );

        let params = params_from_settings(settings, 0);
        let rtao_params_buffer = UniformBuffer::new(device, "RTAO Params", &params);

        let layout = RtaoLayout::new(device);

        let shader_source = shader_source::build_rtao_shader_source();
        let compute_pipeline = pipeline_factory::create_compute_pipeline_with_layout(
            device,
            "RTAO",
            &shader_source,
            layout.bgl(),
        );

        let bind_group = layout.bind(
            device,
            voxels,
            gbuf_depth,
            gbuf_normal,
            &ao_output,
            rtao_params_buffer.buffer(),
        );

        Self {
            compute_pipeline,
            layout,
            bind_group,
            ao_output,
            rtao_params_buffer,
            width,
            height,
        }
    }

    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        gbuf_depth: &GpuTexture,
        gbuf_normal: &GpuTexture,
        voxels: &SharedVoxelBuffers,
        size: [u32; 2],
    ) {
        let [width, height] = size;
        self.ao_output = GpuTexture::new_storage_sampled(
            device,
            "RTAO Output",
            width,
            height,
            wgpu::TextureFormat::R32Float,
        );
        self.width = width;
        self.height = height;
        self.bind_group = self.layout.bind(
            device,
            voxels,
            gbuf_depth,
            gbuf_normal,
            &self.ao_output,
            self.rtao_params_buffer.buffer(),
        );
    }

    pub(crate) fn rebind(
        &mut self,
        device: &wgpu::Device,
        gbuf_depth: &GpuTexture,
        gbuf_normal: &GpuTexture,
        voxels: &SharedVoxelBuffers,
    ) {
        self.bind_group = self.layout.bind(
            device,
            voxels,
            gbuf_depth,
            gbuf_normal,
            &self.ao_output,
            self.rtao_params_buffer.buffer(),
        );
    }

    pub(crate) fn update_params(
        &self,
        queue: &wgpu::Queue,
        settings: &RendererSettings,
        frame_index: u32,
    ) {
        self.rtao_params_buffer
            .write(queue, &params_from_settings(settings, frame_index));
    }
}

fn params_from_settings(settings: &RendererSettings, frame_index: u32) -> RtaoParamsUniform {
    RtaoParamsUniform {
        ao_radius: settings.ao_radius,
        ao_intensity: settings.ao_intensity,
        ao_rays: settings.ao_rays,
        frame_index,
    }
}
