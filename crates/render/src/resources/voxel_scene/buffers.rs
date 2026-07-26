use wgpu::util::DeviceExt;

use capy_core::{Camera, NearVoxelMeshChunk, NearVoxelMeshData, VoxelMeshData, is_water_material};

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
    near_mesh_vertex_buffer: wgpu::Buffer,
    near_mesh_index_buffer: wgpu::Buffer,
    near_mesh_water_index_buffer: wgpu::Buffer,
    near_mesh_instance_buffer: wgpu::Buffer,
    near_mesh_index_count: u32,
    near_mesh_chunks: Vec<NearVoxelMeshChunk>,
    canonical_index_start: u32,
    canonical_index_count: u32,
    near_mesh_water_index_count: u32,
    near_mesh_water_chunks: Vec<NearVoxelMeshChunk>,
    canonical_water_index_start: u32,
    canonical_water_index_count: u32,
    canonical_chunks: Vec<[i32; 3]>,
}

#[derive(Default)]
struct NearMeshIndexLayer {
    indices: Vec<u32>,
    chunks: Vec<NearVoxelMeshChunk>,
    canonical_index_start: u32,
    canonical_index_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct NearMeshGpuVertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) material: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct NearMeshGpuInstance {
    pub(crate) world_offset: [f32; 4],
}

fn split_near_mesh_indices(mesh: &NearVoxelMeshData) -> (NearMeshIndexLayer, NearMeshIndexLayer) {
    let mut opaque = NearMeshIndexLayer::default();
    let mut water = NearMeshIndexLayer::default();

    for chunk in &mesh.chunks {
        split_near_mesh_range(
            mesh,
            chunk.coord,
            chunk.index_start,
            chunk.index_count,
            &mut opaque,
            &mut water,
        );
    }

    opaque.canonical_index_start = opaque.indices.len() as u32;
    water.canonical_index_start = water.indices.len() as u32;
    split_near_mesh_triangles(
        mesh,
        mesh.canonical_index_start,
        mesh.canonical_index_count,
        &mut opaque.indices,
        &mut water.indices,
    );
    opaque.canonical_index_count = opaque.indices.len() as u32 - opaque.canonical_index_start;
    water.canonical_index_count = water.indices.len() as u32 - water.canonical_index_start;

    (opaque, water)
}

fn split_near_mesh_range(
    mesh: &NearVoxelMeshData,
    coord: [i32; 3],
    index_start: u32,
    index_count: u32,
    opaque: &mut NearMeshIndexLayer,
    water: &mut NearMeshIndexLayer,
) {
    let opaque_start = opaque.indices.len() as u32;
    let water_start = water.indices.len() as u32;
    split_near_mesh_triangles(
        mesh,
        index_start,
        index_count,
        &mut opaque.indices,
        &mut water.indices,
    );
    let opaque_count = opaque.indices.len() as u32 - opaque_start;
    let water_count = water.indices.len() as u32 - water_start;
    if opaque_count > 0 {
        opaque.chunks.push(NearVoxelMeshChunk {
            coord,
            index_start: opaque_start,
            index_count: opaque_count,
        });
    }
    if water_count > 0 {
        water.chunks.push(NearVoxelMeshChunk {
            coord,
            index_start: water_start,
            index_count: water_count,
        });
    }
}

fn split_near_mesh_triangles(
    mesh: &NearVoxelMeshData,
    index_start: u32,
    index_count: u32,
    opaque: &mut Vec<u32>,
    water: &mut Vec<u32>,
) {
    let start = index_start as usize;
    let end = start + index_count as usize;
    for triangle in mesh.indices[start..end].chunks_exact(3) {
        let material = mesh.vertices[triangle[0] as usize].material;
        if is_water_material(material) {
            water.extend_from_slice(triangle);
        } else {
            opaque.extend_from_slice(triangle);
        }
    }
}

impl PreparedVoxelSceneUpload {
    pub(crate) fn build(device: &wgpu::Device, mesh: &VoxelMeshData) -> Result<Self> {
        let near_vertices: Vec<NearMeshGpuVertex> = mesh
            .near_mesh
            .vertices
            .iter()
            .map(|vertex| NearMeshGpuVertex {
                position: vertex.position,
                normal: vertex.normal,
                material: u32::from(vertex.material),
            })
            .collect();
        let (opaque_layer, water_layer) = split_near_mesh_indices(&mesh.near_mesh);
        let mut near_instances = Vec::with_capacity(mesh.near_mesh.canonical_chunks.len() + 1);
        near_instances.push(NearMeshGpuInstance {
            world_offset: [0.0; 4],
        });
        near_instances.extend(mesh.near_mesh.canonical_chunks.iter().map(|coord| {
            NearMeshGpuInstance {
                world_offset: [
                    coord[0] as f32 * mesh.chunk_size_xz as f32,
                    coord[1] as f32 * mesh.chunk_size_y as f32,
                    coord[2] as f32 * mesh.chunk_size_xz as f32,
                    0.0,
                ],
            }
        }));
        Ok(Self {
            pool_buffer: create_storage_buffer(device, "Chunk Pool", &mesh.pool_dag)?,
            avg_pool_buffer: create_storage_buffer(device, "Avg Color Pool", &mesh.pool_avg)?,
            indirection_buffer: create_storage_buffer(device, "Indirection", &mesh.indirection)?,
            near_mesh_vertex_buffer: create_mesh_buffer(
                device,
                "Near Mesh Vertices",
                bytemuck::cast_slice(&near_vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            near_mesh_index_buffer: create_mesh_buffer(
                device,
                "Near Mesh Indices",
                bytemuck::cast_slice(&opaque_layer.indices),
                wgpu::BufferUsages::INDEX,
            ),
            near_mesh_water_index_buffer: create_mesh_buffer(
                device,
                "Near Water Mesh Indices",
                bytemuck::cast_slice(&water_layer.indices),
                wgpu::BufferUsages::INDEX,
            ),
            near_mesh_instance_buffer: create_mesh_buffer(
                device,
                "Near Mesh Instances",
                bytemuck::cast_slice(&near_instances),
                wgpu::BufferUsages::VERTEX,
            ),
            near_mesh_index_count: opaque_layer.indices.len() as u32,
            near_mesh_chunks: opaque_layer.chunks,
            canonical_index_start: opaque_layer.canonical_index_start,
            canonical_index_count: opaque_layer.canonical_index_count,
            near_mesh_water_index_count: water_layer.indices.len() as u32,
            near_mesh_water_chunks: water_layer.chunks,
            canonical_water_index_start: water_layer.canonical_index_start,
            canonical_water_index_count: water_layer.canonical_index_count,
            canonical_chunks: mesh.near_mesh.canonical_chunks.clone(),
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
    pub(crate) near_mesh_vertex_buffer: wgpu::Buffer,
    pub(crate) near_mesh_index_buffer: wgpu::Buffer,
    pub(crate) near_mesh_water_index_buffer: wgpu::Buffer,
    pub(crate) near_mesh_instance_buffer: wgpu::Buffer,
    pub(crate) near_mesh_index_count: u32,
    pub(crate) near_mesh_chunks: Vec<NearVoxelMeshChunk>,
    pub(crate) canonical_index_start: u32,
    pub(crate) canonical_index_count: u32,
    pub(crate) near_mesh_water_index_count: u32,
    pub(crate) near_mesh_water_chunks: Vec<NearVoxelMeshChunk>,
    pub(crate) canonical_water_index_start: u32,
    pub(crate) canonical_water_index_count: u32,
    pub(crate) canonical_chunks: Vec<[i32; 3]>,
    pub(crate) chunk_size_xz: u32,
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
            settings.water_enabled && mesh.is_water_at(camera.position.to_array()),
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
            near_mesh_vertex_buffer: upload.near_mesh_vertex_buffer,
            near_mesh_index_buffer: upload.near_mesh_index_buffer,
            near_mesh_water_index_buffer: upload.near_mesh_water_index_buffer,
            near_mesh_instance_buffer: upload.near_mesh_instance_buffer,
            near_mesh_index_count: upload.near_mesh_index_count,
            near_mesh_chunks: upload.near_mesh_chunks,
            canonical_index_start: upload.canonical_index_start,
            canonical_index_count: upload.canonical_index_count,
            near_mesh_water_index_count: upload.near_mesh_water_index_count,
            near_mesh_water_chunks: upload.near_mesh_water_chunks,
            canonical_water_index_start: upload.canonical_water_index_start,
            canonical_water_index_count: upload.canonical_water_index_count,
            canonical_chunks: upload.canonical_chunks,
            chunk_size_xz: mesh.chunk_size_xz,
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
        self.near_mesh_vertex_buffer = upload.near_mesh_vertex_buffer;
        self.near_mesh_index_buffer = upload.near_mesh_index_buffer;
        self.near_mesh_water_index_buffer = upload.near_mesh_water_index_buffer;
        self.near_mesh_instance_buffer = upload.near_mesh_instance_buffer;
        self.near_mesh_index_count = upload.near_mesh_index_count;
        self.near_mesh_chunks = upload.near_mesh_chunks;
        self.canonical_index_start = upload.canonical_index_start;
        self.canonical_index_count = upload.canonical_index_count;
        self.near_mesh_water_index_count = upload.near_mesh_water_index_count;
        self.near_mesh_water_chunks = upload.near_mesh_water_chunks;
        self.canonical_water_index_start = upload.canonical_water_index_start;
        self.canonical_water_index_count = upload.canonical_water_index_count;
        self.canonical_chunks = upload.canonical_chunks;
        self.chunk_size_xz = mesh.chunk_size_xz;

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

fn create_mesh_buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    if contents.is_empty() {
        return device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: 4,
            usage,
            mapped_at_creation: false,
        });
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
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
