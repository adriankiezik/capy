use crate::{
    graphics::{GraphicsError, Result},
    scene::{Mesh, MeshId, MeshInstance, Vertex},
};
use glam::{Mat4, Vec3, Vec4};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use wgpu::util::DeviceExt;

struct Resident {
    source: Arc<Mesh>,
    buffer: wgpu::Buffer,
}

pub(in crate::render) struct WorldRenderer {
    pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    meshes: BTreeMap<MeshId, Resident>,
    visible: Vec<MeshId>,
}

impl WorldRenderer {
    pub(in crate::render) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        Self {
            pipeline: Self::pipeline(device, format, view_layout),
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("world instances"),
                size: 16,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            instance_capacity: 1,
            meshes: BTreeMap::new(),
            visible: Vec::new(),
        }
    }

    fn pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxel surfaces"),
            source: wgpu::ShaderSource::Wgsl(include_str!("voxel.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("voxel layout"),
            bind_group_layouts: &[Some(view_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel greedy surfaces"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![3 => Float32x4],
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::render::renderer::DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    pub(in crate::render) fn reconfigure(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
    ) {
        self.pipeline = Self::pipeline(device, format, view_layout);
    }

    pub(in crate::render) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sources: Vec<MeshInstance>,
        matrix: Mat4,
        eye: Vec3,
    ) -> Result<()> {
        let mut retained = BTreeSet::new();

        let mut draws = Vec::new();

        for MeshInstance {
            id,
            mesh,
            translation,
        } in sources
        {
            if mesh.vertices.is_empty() {
                continue;
            }

            if std::mem::size_of_val(mesh.vertices.as_slice()) as u64
                > device.limits().max_buffer_size
                || mesh.vertices.len() > u32::MAX as usize
            {
                return Err(GraphicsError::ResourceLimit);
            }

            retained.insert(id);

            if !self
                .meshes
                .get(&id)
                .is_some_and(|resident| Arc::ptr_eq(&resident.source, &mesh))
            {
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("immutable voxel leaf"),
                    contents: bytemuck::cast_slice(&mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                self.meshes.insert(
                    id,
                    Resident {
                        source: mesh.clone(),
                        buffer,
                    },
                );
            }

            let origin = mesh.origin + translation;

            let min = mesh.min + origin;

            let max = mesh.max + origin;

            if visible(matrix, min, max) {
                draws.push((
                    eye.distance_squared((min + max) * 0.5),
                    id,
                    origin.extend(0.0).to_array(),
                ));
            }
        }

        self.meshes.retain(|id, _| retained.contains(id));

        draws.sort_by(|a, b| a.0.total_cmp(&b.0));

        if draws.len() > u32::MAX as usize {
            return Err(GraphicsError::ResourceLimit);
        }

        self.visible = draws.iter().map(|d| d.1).collect();

        let instances: Vec<[f32; 4]> = draws.into_iter().map(|d| d.2).collect();

        if instances.len() > self.instance_capacity {
            let capacity = instances
                .len()
                .checked_next_power_of_two()
                .ok_or(GraphicsError::ResourceLimit)?;

            if capacity as u64 > device.limits().max_buffer_size / 16 {
                return Err(GraphicsError::ResourceLimit);
            }

            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("world instances"),
                size: capacity as u64 * 16,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = capacity;
        }

        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }

        Ok(())
    }

    pub(in crate::render) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, view: &wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);

        pass.set_bind_group(0, view, &[]);

        pass.set_vertex_buffer(1, self.instances.slice(..));

        for (index, id) in self.visible.iter().enumerate() {
            if let Some(mesh) = self.meshes.get(id) {
                pass.set_vertex_buffer(0, mesh.buffer.slice(..));

                pass.draw(
                    0..mesh.source.vertices.len() as u32,
                    index as u32..index as u32 + 1,
                );
            }
        }
    }
}

fn visible(matrix: Mat4, min: Vec3, max: Vec3) -> bool {
    let corners: [Vec4; 8] = std::array::from_fn(|i| {
        matrix
            * Vec4::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
                1.0,
            )
    });

    !(0..6).any(|plane| {
        corners.iter().all(|p| match plane {
            0 => p.x < -p.w,
            1 => p.x > p.w,
            2 => p.y < -p.w,
            3 => p.y > p.w,
            4 => p.z < 0.0,
            _ => p.z > p.w,
        })
    })
}
