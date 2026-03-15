use wgpu::util::DeviceExt;

use capy_core::{Camera, VoxelMeshData};

use crate::camera::CameraUniform;
use crate::resources::RendererSettings;
use crate::settings::{RenderSettingsUniform, StreamingInfoUniform, to_render_settings_uniform};
use crate::uniform_buffer::UniformBuffer;

pub(crate) struct VoxelSceneBuffers {
    pub(crate) camera_buffer: UniformBuffer<CameraUniform>,
    pub(crate) render_settings_buffer: UniformBuffer<RenderSettingsUniform>,
    pub(crate) streaming_info_buffer: UniformBuffer<StreamingInfoUniform>,
    pub(crate) pool_buffer: wgpu::Buffer,
    pub(crate) avg_pool_buffer: wgpu::Buffer,
    pub(crate) indirection_buffer: wgpu::Buffer,
}

impl VoxelSceneBuffers {
    pub(crate) fn new(
        device: &wgpu::Device,
        mesh: &VoxelMeshData,
        camera: &Camera,
        width: u32,
        height: u32,
        settings: &RendererSettings,
    ) -> Self {
        let camera_uniform = CameraUniform::from_camera(camera, width, height, settings.lod_bias);
        let camera_buffer = UniformBuffer::new(device, "Camera Uniform", &camera_uniform);

        let render_settings_uniform = to_render_settings_uniform(settings);
        let render_settings_buffer =
            UniformBuffer::new(device, "Render Settings Uniform", &render_settings_uniform);

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

        let indirection_data: [u32; 4] = [mesh.world_size, mesh.root_offset, mesh.depth, 0];
        let indirection_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Indirection"),
            contents: bytemuck::cast_slice(&indirection_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

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
        let streaming_info_buffer = UniformBuffer::new(device, "Streaming Info", &streaming_info);

        Self {
            camera_buffer,
            render_settings_buffer,
            streaming_info_buffer,
            pool_buffer,
            avg_pool_buffer,
            indirection_buffer,
        }
    }

    pub(crate) fn upload_camera(
        &self,
        queue: &wgpu::Queue,
        camera: &Camera,
        width: u32,
        height: u32,
        lod_bias: f32,
    ) {
        let uniform = CameraUniform::from_camera(camera, width, height, lod_bias);
        self.camera_buffer.write(queue, &uniform);
    }

    pub(crate) fn upload_render_settings(&self, queue: &wgpu::Queue, settings: &RendererSettings) {
        self.render_settings_buffer
            .write(queue, &to_render_settings_uniform(settings));
    }

    pub(crate) fn shared_voxel_buffers(&self) -> crate::resources::SharedVoxelBuffers {
        crate::resources::SharedVoxelBuffers {
            pool_buffer: self.pool_buffer.clone(),
            avg_pool_buffer: self.avg_pool_buffer.clone(),
            indirection_buffer: self.indirection_buffer.clone(),
            streaming_info_buffer: self.streaming_info_buffer.buffer().clone(),
            render_settings_buffer: self.render_settings_buffer.buffer().clone(),
        }
    }
}
