use super::{Hit, Mesh, Replica, Result, Target};
use crate::{Aabb, replica::world::VOXEL_SIZE};
use glam::{IVec3, Vec3};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MeshId {
    Static([i32; 3]),
    Body(u64, [i32; 3]),
    Object(u64),
}

pub(crate) struct MeshInstance {
    pub(crate) id: MeshId,
    pub(crate) mesh: Arc<Mesh>,
    pub(crate) world_origin: Vec3,
    pub(crate) surface_origin: Vec3,
}

impl Replica {
    pub(crate) fn render_geometry(&self) -> Result<Vec<MeshInstance>> {
        let mut instances = Vec::new();

        let mut available = self.settings.max_mesh_vertices;

        for (&key, source) in self.meshes.iter() {
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
                continue;
            }

            available -= mesh.vertices.len();

            let origin =
                (IVec3::from_array(key) * self.settings.render_leaf_edge).as_vec3() * VOXEL_SIZE;

            instances.push(MeshInstance {
                id: MeshId::Static(key),
                world_origin: origin,
                surface_origin: origin,
                mesh,
            });
        }

        for body in &self.bodies {
            for (&key, source) in &body.geometry.meshes {
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
                    continue;
                }

                available -= mesh.vertices.len();

                let origin = (IVec3::from_array(key) * self.settings.render_leaf_edge).as_vec3()
                    * VOXEL_SIZE;

                instances.push(MeshInstance {
                    id: MeshId::Body(body.id, key),
                    world_origin: self.translation(body) + origin,
                    surface_origin: origin,
                    mesh,
                });
            }
        }

        for (&id, object) in &self.objects {
            if object.mesh.vertices.len() > available {
                continue;
            }

            available -= object.mesh.vertices.len();

            instances.push(MeshInstance {
                id: MeshId::Object(id),
                mesh: object.mesh.clone(),
                world_origin: object.description.bounds.min,
                surface_origin: Vec3::ZERO,
            });
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
