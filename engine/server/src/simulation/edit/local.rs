use super::patch::{Input, Patch, dirty_keys, same_leaf, wake_bounds};
use crate::{
    simulation::{
        Body, Result, Simulation, SimulationError, body::GeometryEdit, connectivity::Connectivity,
        work::Work,
    },
    world::{
        LEAF_VOXELS, Leaf, LeafCoord, VOXEL_SIZE, Voxel, VoxelCoord, WorldRead, address, coordinate,
    },
};
use glam::IVec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(in crate::simulation) struct LocalPatch {
    target: LocalTarget,
    dependencies: BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
    changes: BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
}

enum LocalTarget {
    Static { base: WorldRead },
    Dynamic { id: u64, geometry: GeometryEdit },
}

impl Input {
    pub(super) async fn prepare_local(
        self,
        voxels: Vec<VoxelCoord>,
        cache: &mut Connectivity,
    ) -> Result<Patch> {
        let (world, limits) = match &self {
            Self::Static { world, limits, .. } | Self::Dynamic { world, limits, .. } => {
                (world, *limits)
            }
        };

        let source = |key| match &self {
            Self::Static { world, .. } => world.leaf(key),
            Self::Dynamic { geometry, .. } => geometry.leaves.get(&key),
        };

        let mut values = BTreeMap::new();

        let mut dirty = BTreeSet::new();

        let mut work = Work::default();

        for voxel in voxels {
            work.checkpoint().await;

            if matches!(&self, Self::Static { .. }) && !world.bounds().contains(voxel) {
                return Err(SimulationError::Invalid);
            }

            let (key, index) = address(voxel);

            let Some(leaf) = source(key) else { continue };

            if leaf.voxel(index).is_empty() {
                continue;
            }

            values.entry(key).or_insert_with(|| leaf.decode())[index] = Voxel::EMPTY;

            dirty_keys(&mut dirty, voxel, voxel, limits.edge);
        }

        let changes: BTreeMap<_, _> = values
            .into_iter()
            .map(|(key, values)| (key, Leaf::encode(&values).map(Arc::new)))
            .collect();

        if changes.is_empty() {
            return Ok(Patch::Unchanged);
        }

        if !cache.preserves(world, &changes, source).await? {
            return Ok(Patch::StructuralRequired);
        }

        let mut dependencies = BTreeMap::new();

        for key in &dirty {
            work.checkpoint().await;

            let min = address(IVec3::from_array(*key) * limits.edge - IVec3::ONE).0;

            let max = address((IVec3::from_array(*key) + IVec3::ONE) * limits.edge).0;

            for z in min[2]..=max[2] {
                for y in min[1]..=max[1] {
                    for x in min[0]..=max[0] {
                        dependencies.insert([x, y, z], source([x, y, z]).cloned());
                    }
                }
            }
        }

        for &key in changes.keys() {
            for direction in [
                IVec3::ZERO,
                IVec3::X,
                IVec3::NEG_X,
                IVec3::Y,
                IVec3::NEG_Y,
                IVec3::Z,
                IVec3::NEG_Z,
            ] {
                let key = (IVec3::from_array(key) + direction).to_array();

                dependencies.insert(key, source(key).cloned());
            }
        }

        let target = match &self {
            Self::Static { world, .. } => LocalTarget::Static {
                base: world.clone(),
            },
            Self::Dynamic { id, geometry, .. } => LocalTarget::Dynamic {
                id: *id,
                geometry: GeometryEdit::prepare(geometry, cache, world, &changes).await?,
            },
        };

        Ok(Patch::Local(Box::new(LocalPatch {
            target,
            dependencies,
            changes,
        })))
    }
}

impl LocalPatch {
    pub(super) fn publish(&mut self, scene: &mut Simulation) -> Result<Vec<Body>> {
        let translation =
            match &mut self.target {
                LocalTarget::Dynamic { id, geometry } => {
                    let index = scene
                        .bodies
                        .binary_search_by_key(id, |body| body.id)
                        .map_err(|_| SimulationError::StaleEdit)?;

                    let body = &mut scene.bodies[index];

                    if self.dependencies.iter().any(|(&key, expected)| {
                        !same_leaf(expected.as_ref(), body.geometry.leaves.get(&key))
                    }) {
                        return Err(SimulationError::StaleEdit);
                    }

                    geometry.apply(&mut body.geometry)?;

                    body.translation
                }
                LocalTarget::Static { base } => {
                    if self.dependencies.iter().any(|(&key, expected)| {
                        !same_leaf(expected.as_ref(), scene.world.leaf(key))
                    }) {
                        return Err(SimulationError::StaleEdit);
                    }

                    let world = scene.world.replace_sources(self.changes.clone())?;

                    *base = std::mem::replace(&mut scene.world, world);

                    glam::Vec3::ZERO
                }
            };

        let min = self
            .changes
            .keys()
            .fold(IVec3::splat(i32::MAX), |min, &key| {
                min.min(coordinate(key, 0))
            });

        let max = self
            .changes
            .keys()
            .fold(IVec3::splat(i32::MIN), |max, &key| {
                max.max(coordinate(key, LEAF_VOXELS - 1) + IVec3::ONE)
            });

        wake_bounds(
            scene,
            min.as_vec3() * VOXEL_SIZE + translation,
            max.as_vec3() * VOXEL_SIZE + translation,
        );

        Ok(Vec::new())
    }
}
