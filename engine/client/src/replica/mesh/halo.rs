use crate::replica::world::{LEAF_EDGE, Leaf, Voxel, VoxelCoord};
use glam::IVec3;

#[derive(Default)]
pub(crate) struct Halo {
    origin: IVec3,
    edge: usize,
    voxels: Vec<Voxel>,
}

impl Halo {
    #[cfg(feature = "cpu-bench")]
    pub(crate) fn sample(
        &mut self,
        origin: IVec3,
        edge: i32,
        sample: impl Fn(VoxelCoord) -> Voxel,
    ) {
        let origin = origin - IVec3::ONE;

        let edge = (edge + 2) as usize;

        self.origin = origin;
        self.edge = edge;

        self.voxels.clear();

        self.voxels.reserve(edge * edge * edge);

        for z in 0..edge {
            for y in 0..edge {
                for x in 0..edge {
                    self.voxels
                        .push(sample(origin + IVec3::new(x as i32, y as i32, z as i32)));
                }
            }
        }
    }

    pub(crate) fn fill<'a>(
        &mut self,
        origin: IVec3,
        edge: i32,
        leaf: impl Fn([i32; 3]) -> Option<&'a Leaf>,
    ) {
        let min = origin - IVec3::ONE;

        let max = origin + IVec3::splat(edge + 1);

        let edge = (edge + 2) as usize;

        self.origin = min;
        self.edge = edge;

        self.voxels.resize(edge * edge * edge, Voxel::EMPTY);

        self.voxels.fill(Voxel::EMPTY);

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
                                self.voxels[target + (px - min.x) as usize] =
                                    leaf.voxel(source + (px - base.x) as usize);
                            }
                        }
                    }
                }
            }
        }
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
