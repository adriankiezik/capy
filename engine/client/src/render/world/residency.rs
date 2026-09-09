use super::visibility::{Hierarchy, Order};
use crate::{
    Aabb,
    graphics::{GraphicsError, Result},
    replica::{Mesh, MeshId, MeshInstance},
};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, sync::Arc};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Instance {
    translation: [f32; 4],
    origin: [f32; 4],
}

impl Instance {
    const SIZE: u64 = std::mem::size_of::<Self>() as u64;

    fn new(world_origin: Vec3, surface_origin: Vec3) -> Self {
        Self {
            translation: world_origin.extend(0.0).to_array(),
            origin: surface_origin.extend(0.0).to_array(),
        }
    }
}

struct Resident {
    id: MeshId,
    source: Arc<Mesh>,
    buffer: wgpu::Buffer,
    origin: Vec3,
}

pub(super) struct Geometry {
    instances: wgpu::Buffer,
    instance_capacity: usize,
    transforms: Vec<Instance>,
    meshes: Vec<Resident>,
    bounds: Vec<Aabb>,
    hierarchy: Hierarchy,
}

impl Geometry {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("world instances"),
                size: Instance::SIZE,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            instance_capacity: 1,
            transforms: Vec::new(),
            meshes: Vec::new(),
            bounds: Vec::new(),
            hierarchy: Hierarchy::default(),
        }
    }

    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mut sources: Vec<MeshInstance>,
    ) -> Result<bool> {
        sources.retain(|source| !source.mesh.vertices.is_empty());

        let changed = sources.len() != self.meshes.len()
            || sources.iter().zip(&self.meshes).any(|(source, resident)| {
                source.id != resident.id || !Arc::ptr_eq(&source.mesh, &resident.source)
            });

        if !changed {
            return Ok(self.update_transforms(queue, &sources));
        }

        if sources.len() > u32::MAX as usize {
            return Err(GraphicsError::ResourceLimit);
        }

        let capacity = sources
            .len()
            .max(1)
            .checked_next_power_of_two()
            .ok_or(GraphicsError::ResourceLimit)?;

        if capacity as u64 > device.limits().max_buffer_size / Instance::SIZE {
            return Err(GraphicsError::ResourceLimit);
        }

        for source in &sources {
            if std::mem::size_of_val(source.mesh.vertices.as_slice()) as u64
                > device.limits().max_buffer_size
                || source.mesh.vertices.len() > u32::MAX as usize
            {
                return Err(GraphicsError::ResourceLimit);
            }
        }

        if sources.len() == self.meshes.len()
            && sources
                .iter()
                .zip(&self.meshes)
                .all(|(source, resident)| source.id == resident.id)
        {
            for (index, (source, resident)) in sources.iter().zip(&mut self.meshes).enumerate() {
                if !Arc::ptr_eq(&source.mesh, &resident.source) {
                    resident.buffer =
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("immutable voxel leaf"),
                            contents: bytemuck::cast_slice(&source.mesh.vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                    resident.source = source.mesh.clone();
                    self.bounds[index] = Aabb {
                        min: source.mesh.min + source.world_origin,
                        max: source.mesh.max + source.world_origin,
                    };
                }
            }

            if !self.update_transforms(queue, &sources) {
                self.hierarchy.refit(&self.bounds);
            }

            return Ok(true);
        }

        let mut previous: BTreeMap<_, _> = std::mem::take(&mut self.meshes)
            .into_iter()
            .map(|resident| (Arc::as_ptr(&resident.source) as usize, resident))
            .collect();

        self.bounds.clear();

        self.transforms.clear();

        for MeshInstance {
            id,
            mesh,
            world_origin: origin,
            surface_origin,
        } in sources
        {
            let resident =
                if let Some(mut resident) = previous.remove(&(Arc::as_ptr(&mesh) as usize)) {
                    resident.id = id;
                    resident.origin = origin;

                    resident
                } else {
                    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("immutable voxel leaf"),
                        contents: bytemuck::cast_slice(&mesh.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                    Resident {
                        id,
                        source: mesh.clone(),
                        buffer,
                        origin,
                    }
                };

            self.bounds.push(Aabb {
                min: mesh.min + origin,
                max: mesh.max + origin,
            });

            self.transforms.push(Instance::new(origin, surface_origin));

            self.meshes.push(resident);
        }

        self.hierarchy.rebuild(&self.bounds);

        if self.transforms.len() > self.instance_capacity {
            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("world instances"),
                size: capacity as u64 * Instance::SIZE,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = capacity;
        }

        if !self.transforms.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.transforms));
        }

        Ok(true)
    }

    fn update_transforms(&mut self, queue: &wgpu::Queue, sources: &[MeshInstance]) -> bool {
        let mut first = sources.len();

        let mut end = 0;

        for (index, (source, resident)) in sources.iter().zip(&mut self.meshes).enumerate() {
            let origin = source.world_origin;

            if origin != resident.origin
                || self.transforms[index].origin != source.surface_origin.extend(0.0).to_array()
            {
                resident.origin = origin;
                self.bounds[index] = Aabb {
                    min: source.mesh.min + origin,
                    max: source.mesh.max + origin,
                };
                self.transforms[index] = Instance::new(origin, source.surface_origin);
                first = first.min(index);
                end = index + 1;
            }
        }

        if first == sources.len() {
            return false;
        }

        queue.write_buffer(
            &self.instances,
            first as u64 * Instance::SIZE,
            bytemuck::cast_slice(&self.transforms[first..end]),
        );

        self.hierarchy.refit(&self.bounds);

        true
    }

    pub(super) fn bounds(&self) -> Option<Aabb> {
        self.hierarchy.bounds()
    }

    pub(super) fn visible(&self, matrix: Mat4, order: Order, output: &mut Vec<usize>) {
        self.hierarchy.visible(matrix, order, output);
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, visible: &[usize]) {
        pass.set_vertex_buffer(1, self.instances.slice(..));

        for &index in visible {
            let mesh = &self.meshes[index];

            pass.set_vertex_buffer(0, mesh.buffer.slice(..));

            pass.draw(
                0..mesh.source.vertices.len() as u32,
                index as u32..index as u32 + 1,
            );
        }
    }
}
