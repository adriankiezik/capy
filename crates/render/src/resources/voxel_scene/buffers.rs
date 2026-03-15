use wgpu::util::DeviceExt;

use capy_core::{Camera, VoxelMeshData};

use crate::camera::CameraUniform;
use crate::resources::RendererSettings;
use crate::settings::{StreamingInfoUniform, to_render_settings_uniform};

pub(crate) struct VoxelSceneBuffers {
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) render_settings_buffer: wgpu::Buffer,
    pub(crate) streaming_info_buffer: wgpu::Buffer,
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
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let render_settings_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Render Settings Uniform"),
            contents: bytemuck::bytes_of(&to_render_settings_uniform(settings)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

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
        let streaming_info_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Streaming Info"),
            contents: bytemuck::bytes_of(&streaming_info),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

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
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub(crate) fn upload_render_settings(&self, queue: &wgpu::Queue, settings: &RendererSettings) {
        queue.write_buffer(
            &self.render_settings_buffer,
            0,
            bytemuck::bytes_of(&to_render_settings_uniform(settings)),
        );
    }

    pub(crate) fn shared_voxel_buffers(&self) -> crate::resources::SharedVoxelBuffers {
        crate::resources::SharedVoxelBuffers {
            pool_buffer: self.pool_buffer.clone(),
            avg_pool_buffer: self.avg_pool_buffer.clone(),
            indirection_buffer: self.indirection_buffer.clone(),
            streaming_info_buffer: self.streaming_info_buffer.clone(),
            render_settings_buffer: self.render_settings_buffer.clone(),
        }
    }
}
