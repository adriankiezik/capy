use super::super::visibility::{Frustum, Hierarchy, Order};
use super::mesh::{Instance, MeshBuffer};
use crate::{
    Aabb,
    graphics::{GraphicsError, Result},
    replica::{MeshId, MeshInstance},
};
use glam::Mat4;
use std::{collections::BTreeMap, sync::Arc};

struct Resident {
    id: MeshId,
    mesh: Arc<MeshBuffer>,
}

pub(in crate::render::world) struct Geometry {
    transforms: Vec<Instance>,
    meshes: Vec<Resident>,
    bounds: Vec<Aabb>,
    hierarchy: Hierarchy,
    changed_bounds: Vec<Aabb>,
    revision: u64,
    transform_revision: u64,
    inputs: Option<Arc<[MeshInstance]>>,
}

impl Geometry {
    pub(in crate::render::world) fn new() -> Self {
        Self {
            transforms: Vec::new(),
            meshes: Vec::new(),
            bounds: Vec::new(),
            hierarchy: Hierarchy::default(),
            changed_bounds: Vec::new(),
            revision: 0,
            transform_revision: 0,
            inputs: None,
        }
    }

    pub(in crate::render::world) fn prepare(
        &mut self,
        device: &wgpu::Device,
        sources: Arc<[MeshInstance]>,
    ) -> Result<bool> {
        self.changed_bounds.clear();

        if self
            .inputs
            .as_ref()
            .is_some_and(|inputs| Arc::ptr_eq(inputs, &sources))
        {
            return Ok(false);
        }

        let changed = sources.len() != self.meshes.len()
            || sources.iter().zip(&self.meshes).any(|(source, resident)| {
                source.id != resident.id || !Arc::ptr_eq(&source.mesh, &resident.mesh.source)
            });

        if !changed {
            self.inputs = Some(sources.clone());

            return Ok(self.update_transforms(&sources));
        }

        let capacity = sources
            .len()
            .max(1)
            .checked_next_power_of_two()
            .ok_or(GraphicsError::ResourceLimit)?;

        if sources.len() > u32::MAX as usize
            || capacity as u64 > device.limits().max_buffer_size / Instance::SIZE
        {
            return Err(GraphicsError::ResourceLimit);
        }

        for source in sources.iter() {
            if std::mem::size_of_val(source.mesh.vertices.as_slice()) as u64
                > device.limits().max_buffer_size
                || source.mesh.vertices.len() > u32::MAX as usize
            {
                return Err(GraphicsError::ResourceLimit);
            }
        }

        let old: BTreeMap<_, _> = self
            .meshes
            .iter()
            .enumerate()
            .map(|(i, resident)| {
                (
                    resident.id,
                    (resident.mesh.clone(), self.bounds[i], self.transforms[i]),
                )
            })
            .collect();

        let mut buffers: BTreeMap<_, _> = self
            .meshes
            .iter()
            .map(|resident| {
                (
                    Arc::as_ptr(&resident.mesh.source) as usize,
                    resident.mesh.clone(),
                )
            })
            .collect();

        self.meshes.clear();

        self.bounds.clear();

        self.transforms.clear();

        for source in sources.iter() {
            let address = Arc::as_ptr(&source.mesh) as usize;

            let mesh = buffers
                .entry(address)
                .or_insert_with(|| Arc::new(MeshBuffer::upload(device, source.mesh.clone())))
                .clone();

            let bounds = transformed_bounds(source);

            match old.get(&source.id) {
                Some((previous, before, transform))
                    if Arc::ptr_eq(previous, &mesh)
                        && *before == bounds
                        && *transform == Instance::new(source) => {}
                Some((_, before, _)) => {
                    self.changed_bounds.push(*before);

                    self.changed_bounds.push(bounds);
                }
                None => self.changed_bounds.push(bounds),
            }

            self.bounds.push(bounds);

            self.transforms.push(Instance::new(source));

            self.meshes.push(Resident {
                id: source.id,
                mesh,
            });
        }

        let ids: std::collections::BTreeSet<_> = sources.iter().map(|s| s.id).collect();

        for (id, (_, before, _)) in old {
            if !ids.contains(&id) {
                self.changed_bounds.push(before);
            }
        }

        self.hierarchy.rebuild(&self.bounds);

        self.inputs = Some(sources);
        self.revision = self.revision.wrapping_add(1);
        self.transform_revision = self.transform_revision.wrapping_add(1);

        Ok(true)
    }

    fn update_transforms(&mut self, sources: &[MeshInstance]) -> bool {
        let mut changed = false;

        for (index, source) in sources.iter().enumerate() {
            let transform = Instance::new(source);

            if self.transforms[index] != transform {
                self.changed_bounds.push(self.bounds[index]);

                self.bounds[index] = transformed_bounds(source);

                self.changed_bounds.push(self.bounds[index]);

                self.transforms[index] = transform;
                changed = true;
            }
        }

        if !changed {
            return false;
        }

        self.transform_revision = self.transform_revision.wrapping_add(1);

        self.hierarchy.refit(&self.bounds);

        true
    }

    #[cfg(feature = "render-bench")]
    pub(in crate::render::world) fn bytes(&self) -> usize {
        let mut seen = std::collections::BTreeSet::new();

        self.meshes
            .iter()
            .filter(|resident| seen.insert(Arc::as_ptr(&resident.mesh) as usize))
            .map(|resident| resident.mesh.bytes)
            .sum()
    }

    pub(super) fn mesh(&self, index: usize) -> &Arc<MeshBuffer> {
        &self.meshes[index].mesh
    }

    pub(super) fn instance(&self, index: usize) -> Instance {
        self.transforms[index]
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(in crate::render::world) fn visibility_revision(&self) -> u64 {
        self.transform_revision
    }

    pub(in crate::render::world) fn changed_in(&self, matrix: Mat4) -> bool {
        let frustum = Frustum::new(matrix);

        self.changed_bounds
            .iter()
            .any(|&bounds| frustum.intersects(bounds))
    }

    pub(in crate::render::world) fn bounds(&self) -> Option<Aabb> {
        self.hierarchy.bounds()
    }

    pub(in crate::render::world) fn visible(
        &self,
        matrix: Mat4,
        order: Order,
        output: &mut Vec<usize>,
    ) {
        self.hierarchy.visible(matrix, order, output);
    }
}

fn transformed_bounds(source: &MeshInstance) -> Aabb {
    let center = (source.mesh.min + source.mesh.max) * 0.5;

    let half = (source.mesh.max - source.mesh.min) * 0.5;

    let center = source.basis.transform_vector3(center) + source.world_origin;

    let half = source.basis.x_axis.truncate().abs() * half.x
        + source.basis.y_axis.truncate().abs() * half.y
        + source.basis.z_axis.truncate().abs() * half.z;

    Aabb {
        min: center - half,
        max: center + half,
    }
}
