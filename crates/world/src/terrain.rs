use noise::{NoiseFn, Perlin};

use capy_core::BakedChunkData;

use crate::bake;
use crate::error::Result;
use crate::voxel_grid::VoxelGrid;

pub const CHUNK_SIZE: u32 = 256;

pub trait TerrainGenerator {
    fn generate(&self, seed: u32) -> Result<BakedChunkData>;
}

pub struct PerlinTerrain {
    pub height_scale: f64,
    pub base_height: f64,
    pub frequency: f64,
    pub detail_ratio: f64,
}

impl Default for PerlinTerrain {
    fn default() -> Self {
        Self {
            height_scale: 0.45,
            base_height: 0.25,
            frequency: 6.0 / 1024.0,
            detail_ratio: 4.0,
        }
    }
}

impl TerrainGenerator for PerlinTerrain {
    fn generate(&self, seed: u32) -> Result<BakedChunkData> {
        let perlin = Perlin::new(seed);

        let cs = CHUNK_SIZE as usize;
        let global_height = CHUNK_SIZE as f64;
        let height_scale = global_height * self.height_scale;
        let base_height = global_height * self.base_height;
        let freq = self.frequency;
        let freq2 = freq * self.detail_ratio;

        let mut col_heights = vec![0u16; cs * cs];
        let total = cs * cs * cs;
        let mut grid = vec![0u8; total];

        for lz in 0..cs {
            let wz = lz as f64;
            let row_base = lz * cs;
            for lx in 0..cs {
                let wx = lx as f64;

                let coarse = perlin.get([wx * freq, wz * freq]);
                let fine = perlin.get([wx * freq2, wz * freq2]);
                let h = base_height + (coarse * 0.7 + fine * 0.3) * height_scale;
                let fill = h.clamp(0.0, CHUNK_SIZE as f64) as usize;
                col_heights[row_base + lx] = fill as u16;

                if fill > 0 {
                    let base_idx = lx + lz * cs * cs;
                    for ly in 0..fill.min(cs) {
                        grid[base_idx + ly * cs] = 1;
                    }
                }
            }
        }

        let voxel_grid = VoxelGrid::new(CHUNK_SIZE, CHUNK_SIZE, CHUNK_SIZE, grid)?;
        bake::bake_chunk(&voxel_grid, Some(&col_heights))
    }
}
