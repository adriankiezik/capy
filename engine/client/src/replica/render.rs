use super::{Hit, Mesh, Replica, Result, Target};
use crate::{Aabb, replica::world::VOXEL_SIZE};
use glam::{IVec3, Mat4, Vec3};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MeshId {
    Static([i32; 3]),
    Body(u64, [i32; 3]),
    Object(u64),
    Model(u64, [i32; 3]),
}

#[derive(Clone)]
pub(crate) struct MeshInstance {
    pub(crate) id: MeshId,
    pub(crate) mesh: Arc<Mesh>,
    pub(crate) world_origin: Vec3,
    pub(crate) basis: Mat4,
    pub(crate) surface_origin: Vec3,
}

impl Replica {
    pub(crate) fn render_geometry(&self) -> Result<Arc<[MeshInstance]>> {
        let interpolating =
            !self.bodies.is_empty() && self.updated.elapsed() < self.settings.update_interval;

        if !interpolating
            && let Some((limit, cached)) = self
                .render_cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            && *limit == self.settings.max_mesh_vertices
        {
            return Ok(cached.clone());
        }

        let mut instances = Vec::new();

        let mut complete = true;

        let mut available = self.settings.max_mesh_vertices;

        for (&key, source) in self.meshes.iter() {
            complete &= source.mesh.get().is_some();

            let Some(mesh) = self.scheduler.mesh(
                key,
                source,
                &self.world,
                &self.world.leaves,
                self.settings.render_leaf_edge,
                self.settings.max_mesh_vertices,
            ) else {
                continue;
            };

            if mesh.vertices.len() > available {
                return Err(super::ReplicaError::Limit("render vertices"));
            }

            available -= mesh.vertices.len();

            let origin =
                (IVec3::from_array(key) * self.settings.render_leaf_edge).as_vec3() * VOXEL_SIZE;

            instances.push(MeshInstance {
                id: MeshId::Static(key),
                world_origin: origin,
                basis: Mat4::IDENTITY,
                surface_origin: origin,
                mesh,
            });
        }

        for body in &self.bodies {
            for (&key, source) in &body.geometry.meshes {
                complete &= source.mesh.get().is_some();

                let Some(mesh) = self.scheduler.mesh(
                    key,
                    source,
                    &self.world,
                    &body.geometry.leaves,
                    self.settings.render_leaf_edge,
                    self.settings.max_mesh_vertices,
                ) else {
                    continue;
                };

                if mesh.vertices.len() > available {
                    return Err(super::ReplicaError::Limit("render vertices"));
                }

                available -= mesh.vertices.len();

                let origin = (IVec3::from_array(key) * self.settings.render_leaf_edge).as_vec3()
                    * VOXEL_SIZE;

                instances.push(MeshInstance {
                    id: MeshId::Body(body.id, key),
                    world_origin: self.translation(body) + origin,
                    basis: Mat4::IDENTITY,
                    surface_origin: origin,
                    mesh,
                });
            }
        }

        for (&id, object) in &self.objects {
            if object.mesh.vertices.len() > available {
                return Err(super::ReplicaError::Limit("render vertices"));
            }

            available -= object.mesh.vertices.len();

            instances.push(MeshInstance {
                id: MeshId::Object(id),
                mesh: object.mesh.clone(),
                world_origin: object.description.bounds.min,
                basis: Mat4::IDENTITY,
                surface_origin: Vec3::ZERO,
            });
        }

        let mut models = std::collections::BTreeSet::new();

        for instance in &self.voxel_instances {
            if models.insert(Arc::as_ptr(&instance.model) as usize) {
                available = available
                    .checked_sub(instance.model.vertices)
                    .ok_or(super::ReplicaError::Limit("render vertices"))?;
            }

            let matrix = instance.matrix();

            let basis = Mat4::from_scale_rotation_translation(
                Vec3::splat(instance.scale),
                instance.rotation,
                Vec3::ZERO,
            );

            for &(key, origin, ref mesh) in &instance.model.leaves {
                instances.push(MeshInstance {
                    id: MeshId::Model(instance.id, key),
                    mesh: mesh.clone(),
                    world_origin: matrix.transform_point3(origin),
                    surface_origin: origin,
                    basis,
                });
            }
        }

        instances.retain(|instance| !instance.mesh.vertices.is_empty());

        let instances: Arc<[MeshInstance]> = instances.into();

        if !interpolating && complete {
            *self.render_cache.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((self.settings.max_mesh_vertices, instances.clone()));
        }

        Ok(instances)
    }

    pub(crate) fn selection_bounds(&self, hit: Hit) -> Option<Aabb> {
        let min = match hit.target {
            Target::Static(voxel) => voxel.as_vec3() * VOXEL_SIZE,
            Target::Dynamic { body, voxel } => {
                let body = self.bodies.iter().find(|b| b.id == body)?;

                voxel.as_vec3() * VOXEL_SIZE + self.translation(body)
            }
        };

        Some(Aabb {
            min,
            max: min + Vec3::splat(VOXEL_SIZE),
        })
    }
}
