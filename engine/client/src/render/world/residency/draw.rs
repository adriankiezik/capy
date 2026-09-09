use super::{
    geometry::Geometry,
    mesh::{Instance, MeshBuffer},
};
use std::sync::Arc;

pub(in crate::render::world) struct DrawList {
    instances: wgpu::Buffer,
    capacity: usize,
    indices: Vec<usize>,
    groups: Vec<(Arc<MeshBuffer>, std::ops::Range<u32>)>,
    geometry_revision: u64,
    transform_revision: u64,
    visible: Vec<usize>,
}

impl DrawList {
    pub(in crate::render::world) fn new(device: &wgpu::Device) -> Self {
        Self {
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("visible instances"),
                size: Instance::SIZE,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            capacity: 1,
            indices: Vec::new(),
            groups: Vec::new(),
            visible: Vec::new(),
            geometry_revision: u64::MAX,
            transform_revision: u64::MAX,
        }
    }

    pub(in crate::render::world) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geometry: &Geometry,
        visible: &[usize],
    ) -> bool {
        let commands_changed =
            self.geometry_revision != geometry.revision() || self.visible != visible;

        if commands_changed {
            let mut lookup = std::collections::HashMap::new();

            let mut groups: Vec<(Arc<MeshBuffer>, Vec<usize>)> = Vec::new();

            for &index in visible {
                let mesh = geometry.mesh(index);

                let address = Arc::as_ptr(mesh) as usize;

                let group = *lookup.entry(address).or_insert_with(|| {
                    groups.push((mesh.clone(), Vec::new()));

                    groups.len() - 1
                });

                groups[group].1.push(index);
            }

            self.indices.clear();

            self.groups.clear();

            for (mesh, indices) in groups {
                let start = self.indices.len() as u32;

                self.indices.extend(indices);

                self.groups.push((mesh, start..self.indices.len() as u32));
            }

            if visible.len() > self.capacity {
                self.capacity = visible.len();
                self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("visible instances"),
                    size: self.capacity as u64 * Instance::SIZE,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }

            self.visible.clear();

            self.visible.extend_from_slice(visible);

            self.geometry_revision = geometry.revision();
        }

        if (commands_changed || self.transform_revision != geometry.visibility_revision())
            && !self.indices.is_empty()
        {
            let transforms: Vec<_> = self.indices.iter().map(|&i| geometry.instance(i)).collect();

            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&transforms));
        }

        self.transform_revision = geometry.visibility_revision();

        commands_changed
    }

    pub(in crate::render::world) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(1, self.instances.slice(..));

        for (mesh, instances) in &self.groups {
            pass.set_vertex_buffer(0, mesh.buffer.slice(..));

            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);

            pass.draw_indexed(0..mesh.source.vertices.len() as u32, 0, instances.clone());
        }
    }

    pub(in crate::render::world) fn bundle<'a>(
        &'a self,
        encoder: &mut wgpu::RenderBundleEncoder<'a>,
    ) {
        encoder.set_vertex_buffer(1, self.instances.slice(..));

        for (mesh, instances) in &self.groups {
            encoder.set_vertex_buffer(0, mesh.buffer.slice(..));

            encoder.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);

            encoder.draw_indexed(0..mesh.source.vertices.len() as u32, 0, instances.clone());
        }
    }
}
