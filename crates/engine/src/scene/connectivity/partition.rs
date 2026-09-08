use crate::{
    scene::{Result, SceneError},
    world::{LEAF_EDGE, LEAF_VOXELS, Leaf, StructureId, VOXEL_SIZE, WorldRead},
};
use glam::IVec3;
use std::sync::Arc;

pub(super) const EMPTY: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Mask([u64; LEAF_VOXELS / 64]);

impl Mask {
    pub(super) fn insert(&mut self, index: usize) {
        self.0[index / 64] |= 1 << (index % 64);
    }

    pub(super) fn contains(self, index: usize) -> bool {
        self.0[index / 64] & (1 << (index % 64)) != 0
    }

    pub(super) fn extend(&mut self, other: Self) {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            *a |= b;
        }
    }

    pub(super) fn count(self) -> usize {
        self.0.iter().map(|word| word.count_ones() as usize).sum()
    }
}

#[derive(Debug)]
pub(super) struct Component {
    pub(super) structure: StructureId,
    pub(super) cells: Mask,
    pub(super) faces: [u64; 6],
    pub(super) min_y: i32,
    pub(super) down: Vec<u16>,
    pub(super) up: Vec<u16>,
}

#[derive(Debug)]
pub(in crate::scene) struct Metrics {
    pub(in crate::scene) min: IVec3,
    pub(in crate::scene) max: IVec3,
    pub(in crate::scene) mass: f32,
    pub(in crate::scene) bottom: Vec<u16>,
}

#[derive(Debug)]
pub(super) struct Partition {
    pub(super) labels: Box<[u16; LEAF_VOXELS]>,
    pub(super) components: Vec<Component>,
    pub(super) occupied: Mask,
    pub(super) metrics: Arc<Metrics>,
}

impl Partition {
    pub(super) fn new(world: &WorldRead, source: &Leaf) -> Result<Self> {
        let mut metrics = Metrics {
            min: IVec3::splat(i32::MAX),
            max: IVec3::splat(i32::MIN),
            mass: 0.0,
            bottom: Vec::new(),
        };

        let mut structures = [StructureId(0); LEAF_VOXELS];

        for (index, structure) in structures.iter_mut().enumerate() {
            let voxel = source.voxel(index);

            if !voxel.is_empty() {
                let p = IVec3::new(
                    (index % 8) as i32,
                    (index / 8 % 8) as i32,
                    (index / 64) as i32,
                );

                metrics.min = metrics.min.min(p);
                metrics.max = metrics.max.max(p + IVec3::ONE);
                metrics.mass += world
                    .material(voxel.material)
                    .ok_or(SceneError::Invalid)?
                    .density
                    * VOXEL_SIZE.powi(3);

                if index / 8 % 8 == 0 || source.voxel(index - 8).is_empty() {
                    metrics.bottom.push(index as u16);
                }

                *structure = world
                    .owner(voxel.owner)
                    .ok_or(SceneError::Invalid)?
                    .structure;
            }
        }

        let mut result = Self {
            labels: Box::new([EMPTY; LEAF_VOXELS]),
            components: Vec::new(),
            occupied: Mask::default(),
            metrics: Arc::new(metrics),
        };

        let mut pending = Vec::new();

        for start in 0..LEAF_VOXELS {
            if structures[start].0 == 0 || result.labels[start] != EMPTY {
                continue;
            }

            let label = result.components.len() as u16;

            let mut component = Component {
                structure: structures[start],
                cells: Mask::default(),
                faces: [0; 6],
                min_y: LEAF_EDGE,
                down: Vec::new(),
                up: Vec::new(),
            };

            result.labels[start] = label;

            pending.push(start);

            while let Some(index) = pending.pop() {
                component.cells.insert(index);

                result.occupied.insert(index);

                let p = [index % 8, index / 8 % 8, index / 64];

                component.min_y = component.min_y.min(p[1] as i32);

                for axis in 0..3 {
                    let stride = [1, 8, 64][axis];

                    for side in 0..2 {
                        if p[axis] == if side == 0 { 7 } else { 0 } {
                            let bit = p[(axis + 1) % 3] + 8 * p[(axis + 2) % 3];

                            component.faces[axis * 2 + side] |= 1 << bit;
                        } else {
                            let neighbor = if side == 0 {
                                index + stride
                            } else {
                                index - stride
                            };

                            if result.labels[neighbor] == EMPTY
                                && structures[neighbor] == component.structure
                            {
                                result.labels[neighbor] = label;

                                pending.push(neighbor);
                            }
                        }
                    }
                }
            }

            result.components.push(component);
        }

        for index in 0..LEAF_VOXELS {
            let upper = result.labels[index];

            if upper == EMPTY || index / 8 % 8 == 0 {
                continue;
            }

            let lower = result.labels[index - 8];

            if lower != EMPTY && upper != lower {
                result.components[upper as usize].down.push(lower);

                result.components[lower as usize].up.push(upper);
            }
        }

        for component in &mut result.components {
            component.down.sort_unstable();

            component.down.dedup();

            component.up.sort_unstable();

            component.up.dedup();
        }

        Ok(result)
    }
}
