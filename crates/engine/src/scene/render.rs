use super::{Hit, Mesh, Result, Scene, Target};
use crate::{Aabb, world::VOXEL_SIZE};
use glam::Vec3;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MeshId(u64, [i32; 3]);

pub(crate) struct MeshInstance {
    pub(crate) id: MeshId,
    pub(crate) mesh: Arc<Mesh>,
    pub(crate) translation: Vec3,
}

impl Scene {
    pub(crate) fn render_geometry(&self) -> Result<Vec<MeshInstance>> {
        let mut instances = Vec::new();

        let mut available = self.settings.max_mesh_vertices;

        for (&key, source) in &self.meshes {
            let mesh = source.resolve(
                key,
                self.settings.render_leaf_edge,
                |key| self.world.leaf(key).map(Arc::as_ref),
                &self.world,
                available,
            )?;

            available -= mesh.vertices.len();

            instances.push(MeshInstance {
                id: MeshId(0, key),
                mesh,
                translation: Vec3::ZERO,
            });
        }

        for body in &self.bodies {
            for (&key, source) in &body.geometry.meshes {
                let mesh = source.resolve(
                    key,
                    self.settings.render_leaf_edge,
                    |key| body.geometry.leaves.get(&key).map(Arc::as_ref),
                    &self.world,
                    available,
                )?;

                available -= mesh.vertices.len();

                instances.push(MeshInstance {
                    id: MeshId(body.id, key),
                    mesh,
                    translation: body.translation,
                });
            }
        }

        Ok(instances)
    }

    pub(crate) fn selection_bounds(&self, hit: Hit) -> Option<Aabb> {
        let min = match hit.target {
            Target::Static(voxel) => voxel.as_vec3() * VOXEL_SIZE,
            Target::Dynamic { body, voxel } => {
                let body = self.bodies.iter().find(|b| b.id == body)?;

                voxel.as_vec3() * VOXEL_SIZE + body.translation
            }
        };

        Some(Aabb {
            min,
            max: min + Vec3::splat(VOXEL_SIZE),
        })
    }
}
