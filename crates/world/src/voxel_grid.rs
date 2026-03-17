use capy_core::MaterialId;

use crate::error::{Result, WorldError};

#[derive(Clone)]
pub struct VoxelGrid {
    pub data: Vec<MaterialId>,
    pub size_x: u32,
    pub size_y: u32,
    pub size_z: u32,
}

impl VoxelGrid {
    pub fn new(size_x: u32, size_y: u32, size_z: u32, data: Vec<MaterialId>) -> Result<Self> {
        let expected = (size_x as usize) * (size_y as usize) * (size_z as usize);
        if data.len() != expected {
            return Err(WorldError::InvalidGridDimensions {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            data,
            size_x,
            size_y,
            size_z,
        })
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, z: i32, material: MaterialId) {
        if x < 0
            || y < 0
            || z < 0
            || x >= self.size_x as i32
            || y >= self.size_y as i32
            || z >= self.size_z as i32
        {
            return;
        }
        self.data
            [(x as u32 + y as u32 * self.size_x + z as u32 * self.size_x * self.size_y) as usize] =
            material;
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> MaterialId {
        if x < 0
            || y < 0
            || z < 0
            || x >= self.size_x as i32
            || y >= self.size_y as i32
            || z >= self.size_z as i32
        {
            return 0;
        }
        self.data
            [(x as u32 + y as u32 * self.size_x + z as u32 * self.size_x * self.size_y) as usize]
    }

    /// Read a 4×4×4 leaf block starting at (ox, oy, oz).
    /// Returns (occupancy bitmask, material array) matching the bit layout
    /// used by `local_to_bit`: bit = lx + ly*4 + lz*16.
    #[inline]
    pub(crate) fn read_leaf_block(&self, ox: u32, oy: u32, oz: u32) -> (u64, [MaterialId; 64]) {
        let mut mask = 0u64;
        let mut materials = [0 as MaterialId; 64];
        let sx = self.size_x as usize;
        let sxy = sx * self.size_y as usize;

        // Fast path: entire 4×4×4 block is within grid bounds
        if ox + 4 <= self.size_x && oy + 4 <= self.size_y && oz + 4 <= self.size_z {
            for lz in 0..4u32 {
                for ly in 0..4u32 {
                    let base = ox as usize + (oy + ly) as usize * sx + (oz + lz) as usize * sxy;
                    let row = &self.data[base..base + 4];
                    for lx in 0..4u32 {
                        let mat = row[lx as usize];
                        if mat != 0 {
                            let bit = lx + ly * 4 + lz * 16;
                            mask |= 1u64 << bit;
                            materials[bit as usize] = mat;
                        }
                    }
                }
            }
        } else {
            // Fallback: block extends beyond grid, use bounds-checked access
            for lz in 0..4u32 {
                for ly in 0..4u32 {
                    for lx in 0..4u32 {
                        let mat = self.get((ox + lx) as i32, (oy + ly) as i32, (oz + lz) as i32);
                        if mat != 0 {
                            let bit = lx + ly * 4 + lz * 16;
                            mask |= 1u64 << bit;
                            materials[bit as usize] = mat;
                        }
                    }
                }
            }
        }

        (mask, materials)
    }
}
