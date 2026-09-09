use crate::replica::world::{LEAF_EDGE, Leaf, Voxel, VoxelCoord};
use glam::IVec3;

pub(crate) struct Halo {
    origin: IVec3,
    edge: usize,
    voxels: Vec<Voxel>,
}

impl Halo {
    #[cfg(feature = "cpu-bench")]
    pub(crate) fn new(origin: IVec3, edge: i32, sample: impl Fn(VoxelCoord) -> Voxel) -> Self {
        let origin = origin - IVec3::ONE;

        let edge = (edge + 2) as usize;

        let mut voxels = Vec::with_capacity(edge * edge * edge);

        for z in 0..edge {
            for y in 0..edge {
                for x in 0..edge {
                    voxels.push(sample(origin + IVec3::new(x as i32, y as i32, z as i32)));
                }
            }
        }

        Self {
            origin,
            edge,
            voxels,
        }
    }

    pub(crate) fn from_leaves<'a>(
        origin: IVec3,
        edge: i32,
        leaf: impl Fn([i32; 3]) -> Option<&'a Leaf>,
    ) -> Self {
        let min = origin - IVec3::ONE;

        let max = origin + IVec3::splat(edge + 1);

        let edge = (edge + 2) as usize;

        let mut halo = Self {
            origin: min,
            edge,
            voxels: vec![Voxel::EMPTY; edge * edge * edge],
        };

        let first = min.to_array().map(|v| v.div_euclid(LEAF_EDGE));

        let last = (max - IVec3::ONE)
            .to_array()
            .map(|v| v.div_euclid(LEAF_EDGE));

        for z in first[2]..=last[2] {
            for y in first[1]..=last[1] {
                for x in first[0]..=last[0] {
                    let Some(leaf) = leaf([x, y, z]) else {
                        continue;
                    };

                    let base = IVec3::new(x, y, z) * LEAF_EDGE;

                    let start = base.max(min);

                    let end = (base + IVec3::splat(LEAF_EDGE)).min(max);

                    for pz in start.z..end.z {
                        for py in start.y..end.y {
                            let source = ((py - base.y) * LEAF_EDGE
                                + (pz - base.z) * LEAF_EDGE * LEAF_EDGE)
                                as usize;

                            let target =
                                ((py - min.y) as usize + (pz - min.z) as usize * edge) * edge;

                            for px in start.x..end.x {
                                halo.voxels[target + (px - min.x) as usize] =
                                    leaf.voxel(source + (px - base.x) as usize);
                            }
                        }
                    }
                }
            }
        }

        halo
    }

    pub(crate) fn voxel(&self, p: VoxelCoord) -> Voxel {
        let p = (p - self.origin).as_uvec3();

        self.voxels[p.x as usize + self.edge * (p.y as usize + self.edge * p.z as usize)]
    }

    pub(crate) fn occlusion(&self, p: VoxelCoord, axis: usize, sign: i32) -> [u8; 4] {
        let mut normal = IVec3::ZERO;

        normal[axis] = sign;

        [(-1, -1), (1, -1), (1, 1), (-1, 1)].map(|(u, v)| {
            let mut side_u = IVec3::ZERO;

            let mut side_v = IVec3::ZERO;

            side_u[(axis + 1) % 3] = u;
            side_v[(axis + 2) % 3] = v;

            let occupied = |offset| u8::from(!self.voxel(p + normal + offset).is_empty());

            let a = occupied(side_u);

            let b = occupied(side_v);

            if a + b == 2 {
                0
            } else {
                3 - a - b - occupied(side_u + side_v)
            }
        })
    }
}
