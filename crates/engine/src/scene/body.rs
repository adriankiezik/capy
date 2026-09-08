use crate::{
    Aabb,
    scene::{
        Result, SceneError,
        connectivity::Connectivity,
        geometry::{self, MeshSource},
    },
    world::{
        LEAF_EDGE, LEAF_VOXELS, Leaf, LeafCoord, VOXEL_SIZE, Voxel, VoxelCoord, WorldRead, address,
        coordinate,
    },
};
use glam::{IVec3, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug)]
struct Summary {
    source: Arc<Leaf>,
    below: Option<Arc<Leaf>>,
    bottom: Vec<VoxelCoord>,
    bounds: Aabb,
    mass: f32,
}

impl Summary {
    fn new(
        cache: &mut Connectivity,
        world: &WorldRead,
        key: LeafCoord,
        source: Arc<Leaf>,
        below: Option<Arc<Leaf>>,
    ) -> Result<Self> {
        let metrics = cache.metrics(world, &source)?;

        let origin = coordinate(key, 0);

        let bottom = metrics
            .bottom
            .iter()
            .filter_map(|&i| {
                let i = i as usize;

                (!(i / 8).is_multiple_of(8)
                    || below
                        .as_ref()
                        .is_none_or(|leaf| leaf.voxel(i + 56).is_empty()))
                .then(|| coordinate(key, i))
            })
            .collect();

        Ok(Self {
            source,
            below,
            bottom,
            bounds: Aabb {
                min: (origin + metrics.min).as_vec3() * VOXEL_SIZE,
                max: (origin + metrics.max).as_vec3() * VOXEL_SIZE,
            },
            mass: metrics.mass,
        })
    }
}

#[derive(Debug)]
pub(crate) struct BodyGeometry {
    pub(crate) leaves: BTreeMap<LeafCoord, Arc<Leaf>>,
    pub(crate) meshes: BTreeMap<[i32; 3], Arc<MeshSource>>,
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
    pub(crate) mass: f32,
    summaries: BTreeMap<LeafCoord, Arc<Summary>>,
}

impl BodyGeometry {
    pub(super) fn new(
        cache: &mut Connectivity,
        world: &WorldRead,
        leaves: BTreeMap<LeafCoord, Arc<Leaf>>,
        edge: i32,
        previous: Option<&Self>,
    ) -> Result<Arc<Self>> {
        let mut summaries = BTreeMap::new();

        let mut dirty = BTreeSet::new();

        let mut render_keys = BTreeSet::new();

        for (&key, source) in &leaves {
            let below = leaves.get(&(IVec3::from_array(key) - IVec3::Y).to_array());

            let old = previous.and_then(|previous| previous.summaries.get(&key));

            let unchanged = old.is_some_and(|old| Arc::ptr_eq(&old.source, source));

            let summary = if let Some(old) = old.filter(|old| {
                unchanged
                    && match (&old.below, below) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            }) {
                old.clone()
            } else {
                Arc::new(Summary::new(
                    cache,
                    world,
                    key,
                    source.clone(),
                    below.cloned(),
                )?)
            };

            summaries.insert(key, summary);

            render_keys.insert(geometry::key(coordinate(key, 0), edge));

            if !unchanged {
                dirty.insert(key);
            }
        }

        if let Some(previous) = previous {
            dirty.extend(
                previous
                    .leaves
                    .keys()
                    .filter(|key| !leaves.contains_key(*key))
                    .copied(),
            );
        }

        let mut dirty_meshes = BTreeSet::new();

        for key in dirty {
            let min = IVec3::from_array(geometry::key(coordinate(key, 0) - IVec3::ONE, edge));

            let max = IVec3::from_array(geometry::key(
                coordinate(key, LEAF_VOXELS - 1) + IVec3::ONE,
                edge,
            ));

            for z in min.z..=max.z {
                for y in min.y..=max.y {
                    for x in min.x..=max.x {
                        dirty_meshes.insert([x, y, z]);
                    }
                }
            }
        }

        let meshes = render_keys
            .into_iter()
            .map(|key| {
                let source = previous
                    .filter(|_| !dirty_meshes.contains(&key))
                    .and_then(|previous| previous.meshes.get(&key))
                    .cloned()
                    .unwrap_or_else(|| Arc::new(MeshSource::default()));

                (key, source)
            })
            .collect();

        let min = summaries
            .values()
            .fold(Vec3::splat(f32::INFINITY), |min, summary| {
                min.min(summary.bounds.min)
            });

        let max = summaries
            .values()
            .fold(Vec3::splat(f32::NEG_INFINITY), |max, summary| {
                max.max(summary.bounds.max)
            });

        let mass = summaries.values().map(|summary| summary.mass).sum();

        Ok(Arc::new(Self {
            leaves,
            meshes,
            min,
            max,
            mass,
            summaries,
        }))
    }

    pub(super) fn remove(
        &self,
        cache: &mut Connectivity,
        world: &WorldRead,
        voxels: &[VoxelCoord],
        edge: i32,
        max_fragments: usize,
    ) -> Result<Vec<Arc<Self>>> {
        if voxels.is_empty() || voxels.len() > world.root.settings.max_edit_voxels {
            return Err(SceneError::Invalid);
        }

        let mut changes = BTreeMap::new();

        for &voxel in voxels {
            if self.voxel(voxel).is_empty() {
                return Err(SceneError::Invalid);
            }

            let (key, cell) = address(voxel);

            changes
                .entry(key)
                .or_insert_with(|| self.leaves[&key].decode())[cell] = Voxel::EMPTY;
        }

        let mut leaves = self.leaves.clone();

        for (key, values) in changes {
            if let Some(leaf) = Leaf::encode(&values) {
                leaves.insert(key, Arc::new(leaf));
            } else {
                leaves.remove(&key);
            }
        }

        let groups = cache.split(world, voxels, |key| leaves.get(&key))?;

        if groups.as_ref().map_or(1, Vec::len) > max_fragments {
            return Err(SceneError::Limit("dynamic bodies"));
        }

        let sources = if let Some(groups) = groups {
            groups
                .iter()
                .map(|group| cache.extract(world, group, |key| leaves.get(&key)))
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![leaves]
        };

        sources
            .into_iter()
            .map(|leaves| Self::new(cache, world, leaves, edge, Some(self)))
            .collect()
    }

    pub(super) fn prepare_meshes(
        &self,
        world: &WorldRead,
        edge: i32,
        available: &mut usize,
    ) -> Result<()> {
        for (&key, source) in &self.meshes {
            let mesh = source.resolve(
                key,
                edge,
                |key| self.leaves.get(&key).map(Arc::as_ref),
                world,
                *available,
            )?;

            *available -= mesh.vertices.len();
        }

        Ok(())
    }

    pub(super) fn inherit_meshes(
        &mut self,
        world: &WorldRead,
        meshes: &BTreeMap<[i32; 3], Arc<MeshSource>>,
        translation: Vec3,
        edge: i32,
    ) {
        let origin = (translation / VOXEL_SIZE).round().as_ivec3();

        for (&key, mesh) in &mut self.meshes {
            let global = (IVec3::from_array(key) + origin / edge).to_array();

            let Some(source) = meshes.get(&global) else {
                continue;
            };

            let min = address(IVec3::from_array(key) * edge - IVec3::ONE).0;

            let max = address((IVec3::from_array(key) + IVec3::ONE) * edge).0;

            let unchanged = (min[2]..=max[2]).all(|z| {
                (min[1]..=max[1]).all(|y| {
                    (min[0]..=max[0]).all(|x| {
                        let local = [x, y, z];

                        let global = (IVec3::from_array(local) + origin / LEAF_EDGE).to_array();

                        match (world.leaf(global), self.leaves.get(&local)) {
                            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                            (None, None) => true,
                            _ => false,
                        }
                    })
                })
            });

            if unchanged {
                *mesh = source.clone();
            }
        }
    }

    pub(crate) fn voxel(&self, p: VoxelCoord) -> Voxel {
        let (key, index) = address(p);

        self.leaves
            .get(&key)
            .map_or(Voxel::EMPTY, |leaf| leaf.voxel(index))
    }

    #[cfg(test)]
    pub(crate) fn occupied(&self) -> impl Iterator<Item = (VoxelCoord, Voxel)> + '_ {
        self.leaves.iter().flat_map(|(&key, leaf)| {
            (0..LEAF_VOXELS).filter_map(move |i| {
                let voxel = leaf.voxel(i);

                (!voxel.is_empty()).then_some((coordinate(key, i), voxel))
            })
        })
    }

    pub(super) fn bottom_regions(&self) -> impl Iterator<Item = (Aabb, &[VoxelCoord])> {
        self.summaries
            .values()
            .filter(|summary| !summary.bottom.is_empty())
            .map(|summary| (summary.bounds, summary.bottom.as_slice()))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Body {
    pub(crate) id: u64,
    pub(crate) geometry: Arc<BodyGeometry>,
    pub(crate) translation: Vec3,
    pub(crate) velocity: f32,
    pub(crate) sleeping: bool,
}
