use std::collections::HashMap;

use capy_core::BakedChunkData;

use crate::bake;
use crate::error::Result;
use crate::terrain::{CHUNK_XZ, CHUNK_Y};
use crate::voxel_grid::VoxelGrid;

// ---------------------------------------------------------------------------
// Simple deterministic PRNG (xorshift32, no external deps)
// ---------------------------------------------------------------------------

struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed.wrapping_add(1)) // avoid zero state
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() & 0x00FF_FFFF) as f32 / 16_777_216.0
    }
}

// ---------------------------------------------------------------------------
// Stress patterns
// ---------------------------------------------------------------------------

/// Which stress pattern to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StressPattern {
    /// Every-other-voxel filled — max leaf visits per ray, deep traversal.
    Checkerboard3D,
    /// Random 30% fill — worst-case DAG (no dedup), cache thrashing.
    RandomNoise,
    /// Solid block with spherical cavities — stresses RTAO interior rays.
    SwissCheese,
    /// Sparse thin columns — defeats mip skip.
    ThinPillars,
    /// 1-voxel-thick horizontal planes — Y-axis traversal, stack pop stress.
    LayeredPlanes,
    /// 45-degree XZ walls — multi-chunk DDA boundary crossings.
    DiagonalMaze,
}

const ALL_PATTERNS: [StressPattern; 6] = [
    StressPattern::Checkerboard3D,
    StressPattern::RandomNoise,
    StressPattern::SwissCheese,
    StressPattern::ThinPillars,
    StressPattern::LayeredPlanes,
    StressPattern::DiagonalMaze,
];

/// Generate a VoxelGrid + column heights for a stress pattern, then bake.
/// The grid is dropped after baking to limit peak memory to ~128MB per chunk.
pub fn generate_stress_baked(pattern: StressPattern, seed: u32) -> Result<BakedChunkData> {
    let xs = CHUNK_XZ as usize;
    let ys = CHUNK_Y as usize;
    let zs = CHUNK_XZ as usize;
    let total = xs * ys * zs;

    let mut data = vec![0u16; total];
    let mut col_heights = vec![0u16; xs * zs];

    match pattern {
        StressPattern::Checkerboard3D => {
            fill_checkerboard(&mut data, &mut col_heights, xs, ys, zs);
        }
        StressPattern::RandomNoise => {
            fill_random_noise(&mut data, &mut col_heights, xs, ys, zs, seed);
        }
        StressPattern::SwissCheese => {
            fill_swiss_cheese(&mut data, &mut col_heights, xs, ys, zs, seed);
        }
        StressPattern::ThinPillars => {
            fill_thin_pillars(&mut data, &mut col_heights, xs, ys, zs, seed);
        }
        StressPattern::LayeredPlanes => {
            fill_layered_planes(&mut data, &mut col_heights, xs, ys, zs);
        }
        StressPattern::DiagonalMaze => {
            fill_diagonal_maze(&mut data, &mut col_heights, xs, ys, zs);
        }
    }

    let grid = VoxelGrid::new(CHUNK_XZ, CHUNK_Y, CHUNK_XZ, data)?;
    bake::bake_chunk(&grid, Some(&col_heights))
}

/// Generate the default stress world layout: 12 unique chunks (6 patterns × 2 seeds).
///
/// Layout (Y=0 layer):
/// - Row z=-3: chunks x=-3..=2, patterns 0..5, seed=1
/// - Row z= 0: chunks x=-3..=2, patterns 0..5, seed=2
/// - Remaining 1012 grid slots stay canonical flat.
pub fn generate_stress_world() -> Result<HashMap<[i32; 3], BakedChunkData>> {
    let mut chunks = HashMap::with_capacity(12);

    for (i, &pattern) in ALL_PATTERNS.iter().enumerate() {
        let x = i as i32 - 3;

        let baked1 = generate_stress_baked(pattern, 1)?;
        chunks.insert([x, 0, -3], baked1);

        let baked2 = generate_stress_baked(pattern, 2)?;
        chunks.insert([x, 0, 0], baked2);
    }

    Ok(chunks)
}

// ---------------------------------------------------------------------------
// Pattern fill functions
// ---------------------------------------------------------------------------

/// Checkerboard3D: every-other voxel filled in y=0..256.
/// Two alternating materials for visual contrast.
fn fill_checkerboard(data: &mut [u16], col_heights: &mut [u16], xs: usize, _ys: usize, zs: usize) {
    let fill_y = 256usize;
    for z in 0..zs {
        for x in 0..xs {
            let mut max_y = 0u16;
            for y in 0..fill_y {
                if (x ^ y ^ z) & 1 == 0 {
                    let mat = if (x + y + z) % 6 < 3 { 1u16 } else { 9u16 };
                    data[x + y * xs + z * xs * _ys] = mat;
                    max_y = (y + 1) as u16;
                }
            }
            col_heights[x + z * xs] = max_y;
        }
    }
}

/// RandomNoise: 30% fill with random materials 1..12 in y=0..128.
fn fill_random_noise(
    data: &mut [u16],
    col_heights: &mut [u16],
    xs: usize,
    ys: usize,
    zs: usize,
    seed: u32,
) {
    let fill_y = 128usize;
    let mut rng = Rng::new(seed);
    for z in 0..zs {
        for x in 0..xs {
            let mut max_y = 0u16;
            for y in 0..fill_y {
                if rng.next_f32() < 0.3 {
                    let mat = (rng.next_u32() % 12 + 1) as u16;
                    data[x + y * xs + z * xs * ys] = mat;
                    max_y = (y + 1) as u16;
                }
            }
            col_heights[x + z * xs] = max_y;
        }
    }
}

/// SwissCheese: solid y=0..200, then carve 30 spherical cavities (r=8..20).
fn fill_swiss_cheese(
    data: &mut [u16],
    col_heights: &mut [u16],
    xs: usize,
    ys: usize,
    zs: usize,
    seed: u32,
) {
    let fill_y = 200usize;

    // Fill solid
    for z in 0..zs {
        for x in 0..xs {
            for y in 0..fill_y {
                data[x + y * xs + z * xs * ys] = 3; // brown material
            }
            col_heights[x + z * xs] = fill_y as u16;
        }
    }

    // Carve cavities
    let mut rng = Rng::new(seed);
    for _ in 0..30 {
        let cx = (rng.next_u32() % (xs as u32 - 40) + 20) as i32;
        let cy = (rng.next_u32() % 160 + 20) as i32;
        let cz = (rng.next_u32() % (zs as u32 - 40) + 20) as i32;
        let r = (rng.next_u32() % 13 + 8) as i32; // 8..20

        let r2 = r * r;
        for dz in -r..=r {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy + dz * dz <= r2 {
                        let px = cx + dx;
                        let py = cy + dy;
                        let pz = cz + dz;
                        if px >= 0
                            && px < xs as i32
                            && py >= 0
                            && py < fill_y as i32
                            && pz >= 0
                            && pz < zs as i32
                        {
                            data[px as usize + py as usize * xs + pz as usize * xs * ys] = 0;
                        }
                    }
                }
            }
        }
    }

    // Recompute column heights after carving
    for z in 0..zs {
        for x in 0..xs {
            let mut max_y = 0u16;
            for y in (0..fill_y).rev() {
                if data[x + y * xs + z * xs * ys] != 0 {
                    max_y = (y + 1) as u16;
                    break;
                }
            }
            col_heights[x + z * xs] = max_y;
        }
    }
}

/// ThinPillars: 1-2 voxel radius columns spaced 8 apart, y=0..256.
fn fill_thin_pillars(
    data: &mut [u16],
    col_heights: &mut [u16],
    xs: usize,
    ys: usize,
    zs: usize,
    seed: u32,
) {
    let fill_y = 256usize;
    let mut rng = Rng::new(seed);

    let spacing = 8usize;
    for pz in (0..zs).step_by(spacing) {
        for px in (0..xs).step_by(spacing) {
            let radius = (rng.next_u32() % 2 + 1) as i32; // 1 or 2
            let mat = (rng.next_u32() % 10 + 1) as u16;
            let pillar_height = fill_y.min(ys);

            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dz * dz > radius * radius {
                        continue;
                    }
                    let x = px as i32 + dx;
                    let z = pz as i32 + dz;
                    if x < 0 || x >= xs as i32 || z < 0 || z >= zs as i32 {
                        continue;
                    }
                    let xu = x as usize;
                    let zu = z as usize;
                    for y in 0..pillar_height {
                        data[xu + y * xs + zu * xs * ys] = mat;
                    }
                    let h = pillar_height as u16;
                    if h > col_heights[xu + zu * xs] {
                        col_heights[xu + zu * xs] = h;
                    }
                }
            }
        }
    }
}

/// LayeredPlanes: 1-voxel-thick planes at y=0,8,16,...248, alternating materials.
fn fill_layered_planes(data: &mut [u16], col_heights: &mut [u16], xs: usize, ys: usize, zs: usize) {
    let materials = [1u16, 5, 9, 11]; // four alternating materials
    let mut plane_idx = 0usize;

    let mut y = 0usize;
    let mut top_y = 0u16;
    while y < 256 && y < ys {
        let mat = materials[plane_idx % materials.len()];
        for z in 0..zs {
            let base = z * xs * ys + y * xs;
            for x in 0..xs {
                data[base + x] = mat;
            }
        }
        top_y = (y + 1) as u16;
        plane_idx += 1;
        y += 8;
    }

    col_heights.fill(top_y);
}

/// DiagonalMaze: 45-degree XZ walls (2 thick, spaced 16), y=0..128.
fn fill_diagonal_maze(data: &mut [u16], col_heights: &mut [u16], xs: usize, ys: usize, zs: usize) {
    let fill_y = 128usize.min(ys);
    let wall_spacing = 16i32;
    let wall_thickness = 2i32;

    for z in 0..zs {
        for x in 0..xs {
            // Diagonal walls: (x + z) mod spacing < thickness
            let diag = ((x as i32 + z as i32) % wall_spacing + wall_spacing) % wall_spacing;
            // Cross-diagonal walls: (x - z) mod spacing < thickness
            let cross = ((x as i32 - z as i32) % wall_spacing + wall_spacing) % wall_spacing;

            let is_wall = diag < wall_thickness || cross < wall_thickness;
            if is_wall {
                let mat = if diag < wall_thickness { 7u16 } else { 14u16 };
                for y in 0..fill_y {
                    data[x + y * xs + z * xs * ys] = mat;
                }
                let h = fill_y as u16;
                if h > col_heights[x + z * xs] {
                    col_heights[x + z * xs] = h;
                }
            }
        }
    }
}
