use crate::replica::{Mesh, MeshInstance};
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(in crate::render::world) struct SurfaceVertex {
    position: [f32; 3],
    color: [f32; 3],
    shading: u32,
}

impl From<&crate::replica::Vertex> for SurfaceVertex {
    fn from(vertex: &crate::replica::Vertex) -> Self {
        let axis = vertex.normal.iter().position(|v| *v != 0.0).unwrap_or(0);

        let normal = axis as u32 * 2 + u32::from(vertex.normal[axis] > 0.0);

        Self {
            position: vertex.position,
            color: vertex.color,
            shading: normal | ((vertex.occlusion * 3.0).round() as u32) << 3,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(in crate::render::world) struct Instance {
    translation: [f32; 4],
    origin: [f32; 4],
    basis: [[f32; 4]; 3],
}

impl Instance {
    pub(super) const SIZE: u64 = std::mem::size_of::<Self>() as u64;

    pub(super) fn new(source: &MeshInstance) -> Self {
        Self {
            translation: source.world_origin.extend(0.0).to_array(),
            origin: source.surface_origin.extend(0.0).to_array(),
            basis: [
                source.basis.x_axis.to_array(),
                source.basis.y_axis.to_array(),
                source.basis.z_axis.to_array(),
            ],
        }
    }
}

pub(super) struct MeshBuffer {
    pub(super) source: Arc<Mesh>,
    pub(super) buffer: wgpu::Buffer,
    pub(super) indices: wgpu::Buffer,
    #[cfg(feature = "render-bench")]
    pub(super) bytes: usize,
}

impl MeshBuffer {
    pub(super) fn upload(device: &wgpu::Device, source: Arc<Mesh>) -> Self {
        let mut unique = std::collections::HashMap::new();

        let mut vertices = Vec::new();

        let mut indices = Vec::with_capacity(source.vertices.len());

        for vertex in &source.vertices {
            let vertex = SurfaceVertex::from(vertex);

            let key: [u32; 7] = bytemuck::cast(vertex);

            let index = *unique.entry(key).or_insert_with(|| {
                let index = vertices.len() as u32;

                vertices.push(vertex);

                index
            });

            indices.push(index);
        }

        Self {
            source,
            #[cfg(feature = "render-bench")]
            bytes: vertices.len() * std::mem::size_of::<SurfaceVertex>() + indices.len() * 4,
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("immutable voxel vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("immutable voxel indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
        }
    }
}
