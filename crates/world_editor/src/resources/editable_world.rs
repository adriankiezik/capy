use std::collections::HashMap;

use bevy_ecs::resource::Resource;
use capy_core::MaterialId;

/// Size of a leaf brick in each dimension.
const BRICK: u32 = 4;

/// Sparse voxel overrides for a single chunk, stored at leaf-brick (4x4x4) granularity.
/// Absent entries fall back to canonical flat terrain.
#[derive(Default, Clone)]
pub struct EditableChunk {
    /// Key: [bx, by, bz] leaf block coordinate. Value: 64-material array.
    pub bricks: HashMap<[u32; 3], [MaterialId; 64]>,
}

impl EditableChunk {
    /// Read an entire brick. Returns the canonical brick if absent.
    pub fn read_brick(&self, bx: u32, by: u32, bz: u32) -> [MaterialId; 64] {
        if let Some(brick) = self.bricks.get(&[bx, by, bz]) {
            *brick
        } else {
            canonical_brick(by)
        }
    }

    /// Write an entire brick.
    pub fn write_brick(&mut self, bx: u32, by: u32, bz: u32, brick: [MaterialId; 64]) {
        let coord = [bx, by, bz];
        if brick == canonical_brick(by) {
            self.bricks.remove(&coord);
        } else {
            self.bricks.insert(coord, brick);
        }
    }
}

/// Canonical brick for a leaf block row at by.
/// `FLAT_FILL_HEIGHT` divides evenly by `BRICK`, so boundary bricks should not occur.
#[inline]
fn canonical_brick(by: u32) -> [MaterialId; 64] {
    let y_base = by * BRICK;
    if y_base + BRICK <= capy_world::FLAT_FILL_HEIGHT {
        [capy_world::FLAT_FILL_MATERIAL; 64]
    } else if y_base >= capy_world::FLAT_FILL_HEIGHT {
        [0 as MaterialId; 64]
    } else {
        let mut brick = [0 as MaterialId; 64];
        for lz in 0..BRICK {
            for ly in 0..BRICK {
                for lx in 0..BRICK {
                    let vy = y_base + ly;
                    if vy < capy_world::FLAT_FILL_HEIGHT {
                        brick[(lx + ly * BRICK + lz * BRICK * BRICK) as usize] =
                            capy_world::FLAT_FILL_MATERIAL;
                    }
                }
            }
        }
        brick
    }
}

#[derive(Resource, Default)]
pub struct EditableWorld {
    pub chunks: HashMap<[i32; 3], EditableChunk>,
}
