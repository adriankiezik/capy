use std::collections::HashMap;

use bevy_ecs::resource::Resource;
use capy_core::MaterialId;

#[derive(Resource, Default)]
pub struct Clipboard {
    /// Brick data keyed by brick-relative coordinates (origin at [0,0,0]).
    pub bricks: HashMap<[u32; 3], [MaterialId; 64]>,
    /// AABB dimensions in voxels [x, y, z].
    pub size: [u32; 3],
}

impl Clipboard {
    pub fn is_empty(&self) -> bool {
        self.bricks.is_empty()
    }

    /// Rotate contents 90° clockwise around the Y axis (looking down).
    /// Per-voxel transform: `(x, y, z) → (z, y, sx-1-x)`, new size: `[sz, sy, sx]`.
    pub fn rotate_cw_y(&mut self) {
        let [sx, sy, sz] = self.size;
        if sx == 0 || sy == 0 || sz == 0 {
            return;
        }

        let new_size = [sz, sy, sx];
        let mut new_bricks: HashMap<[u32; 3], [MaterialId; 64]> = HashMap::new();

        const BRICK: u32 = 4;

        for (&src_brick_coord, src_data) in &self.bricks {
            for lz in 0..BRICK {
                for ly in 0..BRICK {
                    for lx in 0..BRICK {
                        let bit = (lx + ly * BRICK + lz * BRICK * BRICK) as usize;
                        let mat = src_data[bit];
                        if mat == 0 {
                            continue;
                        }

                        // World-relative voxel coordinates within the clipboard
                        let vx = src_brick_coord[0] * BRICK + lx;
                        let vy = src_brick_coord[1] * BRICK + ly;
                        let vz = src_brick_coord[2] * BRICK + lz;

                        // Skip voxels outside logical size (partial boundary bricks)
                        if vx >= sx || vy >= sy || vz >= sz {
                            continue;
                        }

                        // Rotated: (x, y, z) → (z, y, sx-1-x)
                        let dx = vz;
                        let dy = vy;
                        let dz = sx - 1 - vx;

                        let dest_brick = [dx / BRICK, dy / BRICK, dz / BRICK];
                        let dest_local = [dx % BRICK, dy % BRICK, dz % BRICK];
                        let dest_bit =
                            (dest_local[0] + dest_local[1] * BRICK + dest_local[2] * BRICK * BRICK)
                                as usize;

                        new_bricks.entry(dest_brick).or_insert([0; 64])[dest_bit] = mat;
                    }
                }
            }
        }

        self.bricks = new_bricks;
        self.size = new_size;
    }
}
