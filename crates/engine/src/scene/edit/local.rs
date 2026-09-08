use super::patch::{Input, Patch, dirty_keys, same_leaf, wake_bounds};
use crate::{
    scene::{
        Body, Result, Scene, SceneError,
        body::GeometryEdit,
        connectivity::Connectivity,
        geometry::{MeshSource, MeshSources},
        work::Work,
    },
    world::{
        LEAF_EDGE, LEAF_VOXELS, Leaf, LeafCoord, VOXEL_SIZE, Voxel, VoxelCoord, WorldRead, address,
        coordinate,
    },
};
use glam::IVec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(in crate::scene) struct LocalPatch {
    target: LocalTarget,
    dependencies: BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
    changes: BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
}

enum LocalTarget {
    Static {
        base: WorldRead,
        meshes: MeshSources,
        dirty: BTreeSet<[i32; 3]>,
    },
    Dynamic {
        id: u64,
        geometry: GeometryEdit,
    },
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
                return Err(SceneError::Invalid);
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

        let mut meshes = MeshSources::new();

        let mut available = limits.vertices;

        let target = match &self {
            Self::Static { world, .. } => {
                for &key in &dirty {
                    work.checkpoint().await;

                    let mesh = Arc::new(MeshSource::default());

                    let resolved = mesh
                        .resolve_async(
                            key,
                            limits.edge,
                            |key| {
                                changes
                                    .get(&key)
                                    .map_or_else(|| source(key), Option::as_ref)
                                    .map(Arc::as_ref)
                            },
                            world,
                            available,
                        )
                        .await?;

                    available -= resolved.vertices.len();

                    let count = limits.edge / LEAF_EDGE;

                    let origin = IVec3::from_array(key) * count;

                    let occupied = (0..count).any(|z| {
                        (0..count).any(|y| {
                            (0..count).any(|x| {
                                let key = (origin + IVec3::new(x, y, z)).to_array();

                                changes
                                    .get(&key)
                                    .map_or_else(|| source(key), Option::as_ref)
                                    .is_some()
                            })
                        })
                    });

                    if occupied {
                        meshes.insert(key, mesh);
                    }
                }

                LocalTarget::Static {
                    base: world.clone(),
                    meshes,
                    dirty,
                }
            }
            Self::Dynamic { id, geometry, .. } => LocalTarget::Dynamic {
                id: *id,
                geometry: GeometryEdit::prepare(
                    geometry,
                    cache,
                    world,
                    &changes,
                    &dirty,
                    limits.edge,
                    available,
                )
                .await?,
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
    pub(super) fn publish(&mut self, scene: &mut Scene) -> Result<Vec<Body>> {
        let translation =
            match &mut self.target {
                LocalTarget::Dynamic { id, geometry } => {
                    let index = scene
                        .bodies
                        .binary_search_by_key(id, |body| body.id)
                        .map_err(|_| SceneError::StaleEdit)?;

                    let body = &mut scene.bodies[index];

                    if self.dependencies.iter().any(|(&key, expected)| {
                        !same_leaf(expected.as_ref(), body.geometry.leaves.get(&key))
                    }) {
                        return Err(SceneError::StaleEdit);
                    }

                    geometry.apply(&mut body.geometry)?;

                    body.translation
                }
                LocalTarget::Static {
                    base,
                    meshes: prepared,
                    dirty,
                } => {
                    if self.dependencies.iter().any(|(&key, expected)| {
                        !same_leaf(expected.as_ref(), scene.world.leaf(key))
                    }) {
                        return Err(SceneError::StaleEdit);
                    }

                    let world = scene.world.replace_sources(self.changes.clone())?;

                    let mut meshes = (*scene.meshes).clone();

                    for key in dirty.iter() {
                        if let Some(source) = prepared.get(key) {
                            meshes.insert(*key, source.clone());
                        } else {
                            meshes.remove(key);
                        }
                    }

                    *base = std::mem::replace(&mut scene.world, world);
                    scene.meshes = Arc::new(meshes);

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
