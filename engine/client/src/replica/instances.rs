use super::{Replica, ReplicaError, Result, VoxelInstance};
use std::{collections::BTreeSet, sync::Arc};

impl Replica {
    pub fn replace_render_instances(&mut self, instances: &[VoxelInstance]) -> Result<()> {
        let mut ids = BTreeSet::new();

        let mut models = BTreeSet::new();

        let mut vertices = 0usize;

        let mut leaves = 0usize;

        for instance in instances {
            if !ids.insert(instance.id) {
                return Err(ReplicaError::DuplicateRenderInstance(instance.id));
            }

            for (valid, field) in [
                (
                    instance.translation.is_finite(),
                    "translation (must be finite world units)",
                ),
                (
                    instance.rotation.is_finite() && instance.rotation.is_normalized(),
                    "rotation (must be a finite unit quaternion)",
                ),
                (
                    instance.scale.is_finite() && (0.000001..=1000000.0).contains(&instance.scale),
                    "scale (must be between 0.000001 and 1000000)",
                ),
                (instance.matrix().is_finite(), "transform matrix"),
            ] {
                if !valid {
                    return Err(ReplicaError::InvalidRenderInstance {
                        id: instance.id,
                        field,
                    });
                }
            }

            if models.insert(Arc::as_ptr(&instance.model) as usize) {
                vertices = vertices
                    .checked_add(instance.model.vertices)
                    .ok_or(ReplicaError::Limit("model vertices"))?;
            }

            leaves = leaves
                .checked_add(instance.model.leaves.len())
                .ok_or(ReplicaError::Limit("model instances"))?;
        }

        if vertices > self.settings.max_mesh_vertices || leaves > 1_000_000 {
            return Err(ReplicaError::Limit("model residency"));
        }

        if instances.len() == self.voxel_instances.len()
            && instances.iter().zip(&self.voxel_instances).all(|(a, b)| {
                a.id == b.id
                    && Arc::ptr_eq(&a.model, &b.model)
                    && a.translation == b.translation
                    && a.rotation == b.rotation
                    && a.scale == b.scale
            })
        {
            return Ok(());
        }

        *self
            .render_cache
            .get_mut()
            .unwrap_or_else(|e| e.into_inner()) = None;
        self.voxel_instances = instances.to_vec();

        Ok(())
    }
}
