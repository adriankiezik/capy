use wgpu::util::DeviceExt;

use capy_core::{Camera, VoxelMeshData};

use crate::camera::{CameraUniform, clip_from_world};
use crate::error::{RenderError, Result};
use crate::resources::RendererSettings;
use crate::settings::{
    PreviewParamsUniform, RenderSettingsUniform, SelectionUniform, StreamingInfoUniform,
    to_render_settings_uniform,
};
use crate::uniform_buffer::UniformBuffer;

pub struct PreparedVoxelSceneUpload {
    pool_buffer: wgpu::Buffer,
    avg_pool_buffer: wgpu::Buffer,
    indirection_buffer: wgpu::Buffer,
}

impl PreparedVoxelSceneUpload {
    pub(crate) fn build(device: &wgpu::Device, mesh: &VoxelMeshData) -> Result<Self> {
        Ok(Self {
            pool_buffer: create_storage_buffer(device, "Chunk Pool", &mesh.pool_dag)?,
            avg_pool_buffer: create_storage_buffer(device, "Avg Color Pool", &mesh.pool_avg)?,
            indirection_buffer: create_storage_buffer(device, "Indirection", &mesh.indirection)?,
        })
    }
}

pub(crate) struct VoxelSceneBuffers {
    pub(crate) camera_buffer: UniformBuffer<CameraUniform>,
    pub(crate) render_settings_buffer: UniformBuffer<RenderSettingsUniform>,
    pub(crate) streaming_info_buffer: UniformBuffer<StreamingInfoUniform>,
    pub(crate) preview_params_buffer: UniformBuffer<PreviewParamsUniform>,
    pub(crate) selection_buffer: UniformBuffer<SelectionUniform>,
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
    ) -> Result<Self> {
        let camera_uniform = CameraUniform::from_camera(
            camera,
            width,
            height,
            settings.lod_bias,
            mesh.is_water_at(camera.position.to_array()),
            [0.0, 0.0],
            clip_from_world(camera).to_cols_array(),
            0.0,
        );
        let camera_buffer = UniformBuffer::new(device, "Camera Uniform", &camera_uniform);

        let render_settings_uniform = to_render_settings_uniform(settings);
        let render_settings_buffer =
            UniformBuffer::new(device, "Render Settings Uniform", &render_settings_uniform);

        let upload = PreparedVoxelSceneUpload::build(device, mesh)?;

        let streaming_info = StreamingInfoUniform {
            grid_min_x: mesh.grid_min[0],
            grid_min_y: mesh.grid_min[1],
            grid_min_z: mesh.grid_min[2],
            grid_dim_x: mesh.grid_dim[0],
            grid_dim_y: mesh.grid_dim[1],
            grid_dim_z: mesh.grid_dim[2],
            chunk_size_xz: mesh.chunk_size_xz,
            chunk_size_y: mesh.chunk_size_y,
        };
        let streaming_info_buffer = UniformBuffer::new(device, "Streaming Info", &streaming_info);

        let preview_params = PreviewParamsUniform::inactive();
        let preview_params_buffer = UniformBuffer::new(device, "Preview Params", &preview_params);

        let selection = SelectionUniform::inactive();
        let selection_buffer = UniformBuffer::new(device, "Selection Highlight", &selection);

        Ok(Self {
            camera_buffer,
            render_settings_buffer,
            streaming_info_buffer,
            preview_params_buffer,
            selection_buffer,
            pool_buffer: upload.pool_buffer,
            avg_pool_buffer: upload.avg_pool_buffer,
            indirection_buffer: upload.indirection_buffer,
        })
    }

    pub(crate) fn apply_prepared_upload(
        &mut self,
        queue: &wgpu::Queue,
        mesh: &VoxelMeshData,
        upload: PreparedVoxelSceneUpload,
    ) -> bool {
        self.pool_buffer = upload.pool_buffer;
        self.avg_pool_buffer = upload.avg_pool_buffer;
        self.indirection_buffer = upload.indirection_buffer;

        let streaming_info = StreamingInfoUniform {
            grid_min_x: mesh.grid_min[0],
            grid_min_y: mesh.grid_min[1],
            grid_min_z: mesh.grid_min[2],
            grid_dim_x: mesh.grid_dim[0],
            grid_dim_y: mesh.grid_dim[1],
            grid_dim_z: mesh.grid_dim[2],
            chunk_size_xz: mesh.chunk_size_xz,
            chunk_size_y: mesh.chunk_size_y,
        };
        self.streaming_info_buffer.write(queue, &streaming_info);

        true
    }

    pub(crate) fn upload_camera(
        &self,
        queue: &wgpu::Queue,
        camera: &Camera,
        width: u32,
        height: u32,
        lod_bias: f32,
        camera_underwater: bool,
        jitter: [f32; 2],
        prev_clip_from_world: [f32; 16],
        time: f32,
    ) {
        let uniform = CameraUniform::from_camera(
            camera,
            width,
            height,
            lod_bias,
            camera_underwater,
            jitter,
            prev_clip_from_world,
            time,
        );
        self.camera_buffer.write(queue, &uniform);
    }

    pub(crate) fn upload_render_settings(&self, queue: &wgpu::Queue, settings: &RendererSettings) {
        self.render_settings_buffer
            .write(queue, &to_render_settings_uniform(settings));
    }

    pub(crate) fn upload_preview_params(
        &self,
        queue: &wgpu::Queue,
        data: &capy_core::PreviewGpuData,
    ) {
        let uniform = PreviewParamsUniform::from_gpu_data(data);
        self.preview_params_buffer.write(queue, &uniform);
    }

    pub(crate) fn upload_selection(
        &self,
        queue: &wgpu::Queue,
        data: &capy_core::SelectionHighlight,
    ) {
        self.selection_buffer
            .write(queue, &SelectionUniform::from_highlight(data));
    }

    pub(crate) fn shared_voxel_buffers(&self) -> crate::resources::SharedVoxelBuffers {
        crate::resources::SharedVoxelBuffers {
            pool_buffer: self.pool_buffer.clone(),
            avg_pool_buffer: self.avg_pool_buffer.clone(),
            indirection_buffer: self.indirection_buffer.clone(),
            streaming_info_buffer: self.streaming_info_buffer.buffer().clone(),
            render_settings_buffer: self.render_settings_buffer.buffer().clone(),
            camera_buffer: self.camera_buffer.buffer().clone(),
        }
    }
}

fn create_storage_buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u32],
) -> Result<wgpu::Buffer> {
    validate_storage_buffer_size(device, label, contents)?;

    Ok(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(contents),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        }),
    )
}

fn validate_storage_buffer_size(
    device: &wgpu::Device,
    label: &str,
    contents: &[u32],
) -> Result<()> {
    let bytes = std::mem::size_of_val(contents) as u64;
    let limits = device.limits();
    let max_size = limits
        .max_buffer_size
        .min(limits.max_storage_buffer_binding_size as u64);

    if bytes > max_size {
        return Err(RenderError::BufferTooLarge {
            label: label.to_owned(),
            size: bytes,
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_buffer_size: limits.max_buffer_size,
        });
    }

    Ok(())
}
