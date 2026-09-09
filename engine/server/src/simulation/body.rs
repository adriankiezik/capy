use crate::{
    Aabb,
    simulation::{Result, SimulationError, connectivity::Connectivity, work::Work},
    world::{
        Leaf, LeafCoord, LeafSources, VOXEL_SIZE, Voxel, VoxelCoord, WorldRead, address, coordinate,
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
    cells: [IVec3; 2],
    mass: f64,
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
            cells: [origin + metrics.min, origin + metrics.max],
            mass: metrics.mass,
        })
    }
}

#[derive(Clone, Debug, Default)]
struct Shape {
    extents: [im::OrdSet<(i32, LeafCoord)>; 6],
    mass: f64,
}

impl Shape {
    fn update(&mut self, key: LeafCoord, before: Option<&Summary>, after: Option<&Summary>) {
        for (index, extent) in self.extents.iter_mut().enumerate() {
            if let Some(summary) = before {
                extent.remove(&(summary.cells[index / 3][index % 3], key));
            }

            if let Some(summary) = after {
                extent.insert((summary.cells[index / 3][index % 3], key));
            }
        }

        self.mass +=
            after.map_or(0.0, |summary| summary.mass) - before.map_or(0.0, |summary| summary.mass);
    }

    fn bounds(&self) -> (Vec3, Vec3) {
        let min = Vec3::from_array(std::array::from_fn(|axis| {
            self.extents[axis]
                .get_min()
                .map_or(f32::INFINITY, |&(value, _)| value as f32 * VOXEL_SIZE)
        }));

        let max = Vec3::from_array(std::array::from_fn(|axis| {
            self.extents[axis + 3]
                .get_max()
                .map_or(f32::NEG_INFINITY, |&(value, _)| value as f32 * VOXEL_SIZE)
        }));

        (min, max)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BodyGeometry {
    pub(crate) leaves: LeafSources,
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
    pub(crate) mass: f32,
    summaries: im::OrdMap<LeafCoord, Arc<Summary>>,
    shape: Shape,
}

pub(super) struct GeometryEdit {
    base: Arc<BodyGeometry>,
    prepared: Arc<BodyGeometry>,
    changed: Vec<LeafCoord>,
}

impl GeometryEdit {
    pub(super) async fn prepare(
        base: &Arc<BodyGeometry>,
        cache: &mut Connectivity,
        world: &WorldRead,
        changes: &BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
    ) -> Result<Self> {
        let prepared = base.prepare_local(cache, world, changes).await?;

        Ok(Self {
            base: base.clone(),
            prepared,
            changed: changes.keys().copied().collect(),
        })
    }

    pub(super) fn apply(&mut self, current: &mut Arc<BodyGeometry>) -> Result<()> {
        let geometry = if Arc::ptr_eq(&self.base, current) {
            self.prepared.clone()
        } else {
            current.merge_local(&self.prepared, self.changed.iter().copied())?
        };

        self.base = std::mem::replace(current, geometry);

        Ok(())
    }
}

impl BodyGeometry {
    pub(super) async fn new(
        cache: &mut Connectivity,
        world: &WorldRead,
        leaves: LeafSources,
        previous: Option<&Self>,
    ) -> Result<Arc<Self>> {
        let mut summaries = im::OrdMap::new();

        let mut work = Work::default();

        for (&key, source) in &leaves {
            work.checkpoint().await;

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
        }

        let mut shape = Shape::default();

        for (&key, summary) in &summaries {
            work.checkpoint().await;

            shape.update(key, None, Some(summary));
        }

        let (min, max) = shape.bounds();

        let mass = shape.mass as f32;

        Ok(Arc::new(Self {
            leaves,
            min,
            max,
            mass,
            summaries,
            shape,
        }))
    }

    pub(super) fn remove(
        &self,
        cache: &mut Connectivity,
        world: &WorldRead,
        voxels: &[VoxelCoord],
        max_fragments: usize,
    ) -> Result<Vec<Arc<Self>>> {
        pollster::block_on(self.remove_async(cache, world, voxels, max_fragments))
    }

    pub(super) async fn remove_async(
        &self,
        cache: &mut Connectivity,
        world: &WorldRead,
        voxels: &[VoxelCoord],
        max_fragments: usize,
    ) -> Result<Vec<Arc<Self>>> {
        if voxels.is_empty() || voxels.len() > world.root.settings.max_edit_voxels {
            return Err(SimulationError::Invalid);
        }

        let mut changes = BTreeMap::new();

        for &voxel in voxels {
            if self.voxel(voxel).is_empty() {
                return Err(SimulationError::Invalid);
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

        let groups = cache.split(world, voxels, |key| leaves.get(&key)).await?;

        if groups.as_ref().map_or(1, Vec::len) > max_fragments {
            return Err(SimulationError::Limit("dynamic bodies"));
        }

        let mut sources = Vec::new();

        if let Some(groups) = groups {
            for group in &groups {
                sources.push(cache.extract(world, group, |key| leaves.get(&key)).await?);
            }
        } else {
            sources.push(leaves);
        }

        let mut geometries = Vec::new();

        for leaves in sources {
            geometries.push(Self::new(cache, world, leaves, Some(self)).await?);
        }

        Ok(geometries)
    }

    async fn prepare_local(
        &self,
        cache: &mut Connectivity,
        world: &WorldRead,
        changes: &BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
    ) -> Result<Arc<Self>> {
        let mut geometry = self.clone();

        let mut work = Work::default();

        for (&key, value) in changes {
            work.checkpoint().await;

            if let Some(leaf) = value {
                geometry.leaves.insert(key, leaf.clone());
            } else {
                geometry.leaves.remove(&key);
            }
        }

        let summaries: BTreeSet<_> = changes
            .keys()
            .flat_map(|&key| [key, (IVec3::from_array(key) + IVec3::Y).to_array()])
            .collect();

        for key in summaries {
            work.checkpoint().await;

            let after = geometry
                .leaves
                .get(&key)
                .map(|source| {
                    Summary::new(
                        cache,
                        world,
                        key,
                        source.clone(),
                        geometry
                            .leaves
                            .get(&(IVec3::from_array(key) - IVec3::Y).to_array())
                            .cloned(),
                    )
                    .map(Arc::new)
                })
                .transpose()?;

            geometry.replace_summary(key, after);
        }

        (geometry.min, geometry.max) = geometry.shape.bounds();
        geometry.mass = geometry.shape.mass as f32;

        Ok(Arc::new(geometry))
    }

    fn replace_summary(&mut self, key: LeafCoord, after: Option<Arc<Summary>>) {
        self.shape.update(
            key,
            self.summaries.get(&key).map(Arc::as_ref),
            after.as_deref(),
        );

        if let Some(summary) = after {
            self.summaries.insert(key, summary);
        } else {
            self.summaries.remove(&key);
        }
    }

    fn merge_local(
        &self,
        prepared: &Self,
        changed: impl Iterator<Item = LeafCoord>,
    ) -> Result<Arc<Self>> {
        let mut geometry = self.clone();

        for key in changed {
            if let Some(leaf) = prepared.leaves.get(&key) {
                geometry.leaves.insert(key, leaf.clone());
            } else {
                geometry.leaves.remove(&key);
            }

            for summary in [key, (IVec3::from_array(key) + IVec3::Y).to_array()] {
                geometry.replace_summary(summary, prepared.summaries.get(&summary).cloned());
            }
        }

        (geometry.min, geometry.max) = geometry.shape.bounds();
        geometry.mass = geometry.shape.mass as f32;

        Ok(Arc::new(geometry))
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
            (0..crate::world::LEAF_VOXELS).filter_map(move |i| {
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
