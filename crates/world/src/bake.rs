use capy_core::{BakedChunkData, is_foliage_material};

use crate::dag::reduce_to_dag;
use crate::error::Result;
use crate::sparse64tree::{
    ChunkOccupancy, FlatTree, build_and_serialize_tree_with_heights, build_tree_from_mip,
    tree_to_gpu_data,
};
use crate::tree_patch::extract_leaf_bricks;
use crate::voxel_grid::VoxelGrid;

/// Scan a voxel grid for foliage materials.
/// Returns (y_min, y_max, heightmap) where the heightmap stores a per-column u16
/// offset from `y_min` (0xFFFF = no foliage), packed 2 per u32.
/// `None` means all columns have foliage at the same height.
fn scan_foliage(grid: &VoxelGrid) -> (u32, u32, Option<Vec<u32>>, u32, Option<Vec<u32>>) {
    let sx = grid.size_x;
    let sz = grid.size_z;
    let num_cols = (sx * sz) as usize;

    // Find the highest foliage surface per column. A foliage surface is a solid
    // voxel with the foliage bit set AND air (or world top) directly above it.
    let mut col_surface_y = vec![u32::MAX; num_cols]; // MAX = no foliage
    let mut y_min = u32::MAX;
    let mut y_max = 0u32;

    for z in 0..sz {
        for x in 0..sx {
            let col = (x + z * sx) as usize;
            for y in (0..grid.size_y).rev() {
                let mat = grid.get(x as i32, y as i32, z as i32);
                if mat == 0 {
                    continue;
                }
                if !is_foliage_material(mat) {
                    continue;
                }
                let above_is_air =
                    y + 1 >= grid.size_y || grid.get(x as i32, (y + 1) as i32, z as i32) == 0;
                if above_is_air {
                    col_surface_y[col] = y;
                    y_min = y_min.min(y);
                    y_max = y_max.max(y + 1);
                    break;
                }
            }
        }
    }

    if y_max == 0 {
        return (0, 0, None, 0, None);
    }

    let bands = compute_foliage_y_bands(&col_surface_y);
    let tile_ranges = compute_foliage_tile_y_ranges(&col_surface_y, sx, sz, y_min);
    let (ym, yx, hm) = pack_foliage_heightmap(&col_surface_y, num_cols, y_min, y_max);
    (ym, yx, hm, bands, tile_ranges)
}

/// Full bake: builds occupancy mip from scratch + DAG reduction.
/// Used for initial world load and save.
pub fn bake_chunk(grid: &VoxelGrid, col_heights: Option<&[u16]>) -> Result<BakedChunkData> {
    let (foliage_y_min, foliage_y_max, foliage_bitmap, foliage_y_bands, foliage_tile_y_ranges) =
        scan_foliage(grid);
    let flat = build_and_serialize_tree_with_heights(grid, col_heights);
    let dag = reduce_to_dag(flat);
    let (info, dag_buffer, avg_color_buffer) = tree_to_gpu_data(dag);

    Ok(BakedChunkData {
        dag_buffer,
        avg_color_buffer,
        root_offset: info.root_offset,
        world_size: info.world_size,
        depth: info.depth,
        foliage_y_min,
        foliage_y_max,
        foliage_bitmap,
        foliage_y_bands,
        foliage_tile_y_ranges,
    })
}

/// Fast bake: uses pre-built occupancy mip, skips DAG reduction.
/// Used for interactive editor updates where the mip is maintained incrementally.
pub fn bake_chunk_fast(grid: &VoxelGrid, occ: &ChunkOccupancy) -> Result<BakedChunkData> {
    let (foliage_y_min, foliage_y_max, foliage_bitmap, foliage_y_bands, foliage_tile_y_ranges) =
        scan_foliage(grid);
    let flat = build_tree_from_mip(grid, occ);
    let (info, dag_buffer, avg_color_buffer) = tree_to_gpu_data(flat);

    Ok(BakedChunkData {
        dag_buffer,
        avg_color_buffer,
        root_offset: info.root_offset,
        world_size: info.world_size,
        depth: info.depth,
        foliage_y_min,
        foliage_y_max,
        foliage_bitmap,
        foliage_y_bands,
        foliage_tile_y_ranges,
    })
}

/// Compact a baked chunk by removing dead nodes and deduplicating identical subtrees.
/// This repacks the DAG buffer into a dense, cache-friendly layout.
/// Also recomputes the foliage bitmap from the compacted DAG.
pub fn compact_baked_chunk(baked: BakedChunkData, chunk_size_xz: u32) -> BakedChunkData {
    let flat = FlatTree {
        buffer: baked.dag_buffer,
        avg_color_buf: baked.avg_color_buffer,
        root_offset: baked.root_offset,
        world_size: baked.world_size,
        depth: baked.depth,
    };
    let dag = reduce_to_dag(flat);
    let (info, dag_buffer, avg_color_buffer) = tree_to_gpu_data(dag);
    let mut result = BakedChunkData {
        dag_buffer,
        avg_color_buffer,
        root_offset: info.root_offset,
        world_size: info.world_size,
        depth: info.depth,
        foliage_y_min: 0,
        foliage_y_max: 0,
        foliage_bitmap: None,
        foliage_y_bands: 0,
        foliage_tile_y_ranges: None,
    };
    recompute_foliage_bitmap(&mut result, chunk_size_xz);
    result
}

/// Recompute foliage Y range and per-column heightmap from the DAG leaf bricks.
/// Scans all leaf bricks to find foliage surface voxels (solid + foliage bit + air above)
/// at any depth in each column, not just the topmost solid voxel.
pub fn recompute_foliage_bitmap(baked: &mut BakedChunkData, chunk_size_xz: u32) {
    let bricks = extract_leaf_bricks(baked);
    let cs = chunk_size_xz as usize;
    let num_cols = cs * cs;

    // Build a lookup from brick coordinate to index for cross-brick surface checks.
    let brick_map: std::collections::HashMap<(u32, u32, u32), usize> = bricks
        .iter()
        .enumerate()
        .map(|(i, b)| ((b.bx, b.by, b.bz), i))
        .collect();

    // Find the highest foliage surface voxel per column.
    // A foliage surface = solid voxel with foliage bit AND air directly above.
    let mut col_surface_y: Vec<u32> = vec![u32::MAX; num_cols];
    let mut y_min = u32::MAX;
    let mut y_max = 0u32;

    for brick in &bricks {
        let base_x = brick.bx * BRANCH as u32;
        let base_y = brick.by * BRANCH as u32;
        let base_z = brick.bz * BRANCH as u32;

        for lz in 0..BRANCH as u32 {
            for lx in 0..BRANCH as u32 {
                let world_x = base_x + lx;
                let world_z = base_z + lz;
                if world_x >= chunk_size_xz || world_z >= chunk_size_xz {
                    continue;
                }
                let col = world_x as usize + world_z as usize * cs;
                for ly in 0..BRANCH as u32 {
                    let bit =
                        (lx + ly * BRANCH as u32 + lz * BRANCH as u32 * BRANCH as u32) as usize;
                    let mat = brick.materials[bit];
                    if mat == 0 || !is_foliage_material(mat) {
                        continue;
                    }
                    let vy = base_y + ly;

                    // Skip if we already found a higher foliage surface in this column.
                    if col_surface_y[col] != u32::MAX && vy <= col_surface_y[col] {
                        continue;
                    }

                    // Check if the voxel above is air (this is a surface voxel).
                    let above_is_air = if ly + 1 < BRANCH as u32 {
                        let above_bit =
                            (lx + (ly + 1) * BRANCH as u32 + lz * BRANCH as u32 * BRANCH as u32)
                                as usize;
                        brick.materials[above_bit] == 0
                    } else {
                        match brick_map.get(&(brick.bx, brick.by + 1, brick.bz)) {
                            None => true,
                            Some(&above_idx) => {
                                let above_bit = (lx + lz * BRANCH as u32 * BRANCH as u32) as usize;
                                bricks[above_idx].materials[above_bit] == 0
                            }
                        }
                    };

                    if above_is_air {
                        col_surface_y[col] = vy;
                        y_min = y_min.min(vy);
                        y_max = y_max.max(vy + 1);
                    }
                }
            }
        }
    }

    if y_max == 0 {
        baked.foliage_y_min = 0;
        baked.foliage_y_max = 0;
        baked.foliage_bitmap = None;
        baked.foliage_y_bands = 0;
        baked.foliage_tile_y_ranges = None;
        return;
    }

    let bands = compute_foliage_y_bands(&col_surface_y);
    let tile_ranges =
        compute_foliage_tile_y_ranges(&col_surface_y, chunk_size_xz, chunk_size_xz, y_min);
    let (new_y_min, new_y_max, heightmap) =
        pack_foliage_heightmap(&col_surface_y, num_cols, y_min, y_max);
    baked.foliage_y_min = new_y_min;
    baked.foliage_y_max = new_y_max;
    baked.foliage_bitmap = heightmap;
    baked.foliage_y_bands = bands;
    baked.foliage_tile_y_ranges = tile_ranges;
}

/// Maximum blade tip offset from the foliage surface voxel, in voxels.
/// Must be >= shader GRASS_BLADE_HEIGHT (5.0). +1 for the root being one voxel above surface.
const GRASS_TIP_OFFSET: u32 = 6;

/// Compute the 32-bit Y-occupancy band mask from per-column surface Y values.
/// Each bit covers a 32-voxel vertical band. A bit is set when any blade (root to tip)
/// overlaps that band.
fn compute_foliage_y_bands(col_surface_y: &[u32]) -> u32 {
    let mut bands = 0u32;
    for &sy in col_surface_y {
        if sy != u32::MAX {
            let root = sy + 1; // blade root is one voxel above the foliage surface
            let tip = root + GRASS_TIP_OFFSET;
            let band_lo = (root / 32).min(31);
            let band_hi = (tip / 32).min(31);
            bands |= 1u32 << band_lo;
            if band_hi != band_lo {
                bands |= 1u32 << band_hi;
            }
        }
    }
    bands
}

/// Tile size in voxels for per-tile foliage Y-range data.
const TILE_SIZE: u32 = 8;

/// Compute per-tile min/max foliage surface Y from per-column surface Y values.
/// Each tile is packed as one u32: low 16 bits = min_offset, high 16 bits = max_offset
/// (offsets from `y_min`). Empty tiles use 0xFFFFFFFF.
/// Returns `None` when every tile has the same range (uniform foliage) or no foliage.
fn compute_foliage_tile_y_ranges(
    col_surface_y: &[u32],
    chunk_size_x: u32,
    chunk_size_z: u32,
    y_min: u32,
) -> Option<Vec<u32>> {
    let tiles_x = (chunk_size_x + TILE_SIZE - 1) / TILE_SIZE;
    let tiles_z = (chunk_size_z + TILE_SIZE - 1) / TILE_SIZE;
    let num_tiles = (tiles_x * tiles_z) as usize;
    let mut tile_min = vec![u32::MAX; num_tiles];
    let mut tile_max = vec![0u32; num_tiles];
    let mut tile_has_foliage = vec![false; num_tiles];

    for z in 0..chunk_size_z {
        for x in 0..chunk_size_x {
            let col = (x + z * chunk_size_x) as usize;
            let sy = col_surface_y[col];
            if sy == u32::MAX {
                continue;
            }
            let tx = x / TILE_SIZE;
            let tz = z / TILE_SIZE;
            let tile_idx = (tx + tz * tiles_x) as usize;
            tile_min[tile_idx] = tile_min[tile_idx].min(sy);
            tile_max[tile_idx] = tile_max[tile_idx].max(sy);
            tile_has_foliage[tile_idx] = true;
        }
    }

    let mut foliage_count = 0usize;
    let mut packed = vec![0xFFFF_FFFFu32; num_tiles];
    let mut first_min = u32::MAX;
    let mut first_max = 0u32;
    let mut all_same = true;

    for i in 0..num_tiles {
        if tile_has_foliage[i] {
            let off_min = (tile_min[i] - y_min).min(0xFFFE) as u16;
            let off_max = (tile_max[i] - y_min).min(0xFFFE) as u16;
            packed[i] = (off_min as u32) | ((off_max as u32) << 16);
            if foliage_count == 0 {
                first_min = tile_min[i];
                first_max = tile_max[i];
            } else if tile_min[i] != first_min || tile_max[i] != first_max {
                all_same = false;
            }
            foliage_count += 1;
        }
    }

    // None when: no foliage at all, or ALL tiles have foliage at the same range.
    if foliage_count == 0 || (all_same && foliage_count == num_tiles) {
        return None;
    }

    Some(packed)
}

/// Pack per-column foliage surface Y values into a u16 heightmap (2 values per u32).
/// Returns (y_min, y_max, heightmap). `None` heightmap = all columns at same height.
fn pack_foliage_heightmap(
    col_surface_y: &[u32],
    num_cols: usize,
    y_min: u32,
    y_max: u32,
) -> (u32, u32, Option<Vec<u32>>) {
    // Count foliage columns and check if all are at the same height.
    let mut foliage_count = 0u32;
    let uniform_y = y_max - 1;
    let mut all_uniform = true;

    for col in 0..num_cols {
        if col_surface_y[col] != u32::MAX {
            foliage_count += 1;
            if col_surface_y[col] != uniform_y {
                all_uniform = false;
            }
        }
    }

    if foliage_count == 0 {
        return (0, 0, None);
    }

    // All columns at the same height — use sentinel (no heightmap needed).
    if all_uniform && foliage_count == num_cols as u32 {
        return (y_min, y_max, None);
    }

    // Pack per-column u16 offsets: surface_y - y_min, or 0xFFFF for no foliage.
    let heightmap_words = (num_cols + 1) / 2;
    let mut heightmap = vec![0xFFFFFFFFu32; heightmap_words]; // all 0xFFFF = no foliage

    for col in 0..num_cols {
        if col_surface_y[col] != u32::MAX {
            let offset = (col_surface_y[col] - y_min).min(0xFFFE) as u16;
            let word_idx = col / 2;
            let half_shift = (col % 2) * 16;
            // Clear the halfword slot (was 0xFFFF) and set the offset.
            heightmap[word_idx] &= !(0xFFFFu32 << half_shift);
            heightmap[word_idx] |= (offset as u32) << half_shift;
        }
    }

    (y_min, y_max, Some(heightmap))
}

const BRANCH: u32 = 4;

#[cfg(test)]
mod tests {
    use super::{TILE_SIZE, compute_foliage_tile_y_ranges, pack_foliage_heightmap};

    #[test]
    fn foliage_heightmap_keeps_large_vertical_offsets() {
        let cols = [100u32, u32::MAX, 500u32, 900u32];
        let (y_min, y_max, heightmap) = pack_foliage_heightmap(&cols, cols.len(), 100, 901);
        let words = heightmap.expect("non-uniform foliage should produce a heightmap");

        assert_eq!(y_min, 100);
        assert_eq!(y_max, 901);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0] & 0xFFFF, 0);
        assert_eq!((words[0] >> 16) & 0xFFFF, 0xFFFF);
        assert_eq!(words[1] & 0xFFFF, 400);
        assert_eq!((words[1] >> 16) & 0xFFFF, 800);
    }

    #[test]
    fn uniform_full_foliage_still_uses_bitmap_sentinel() {
        let cols = [256u32; 4];
        let (y_min, y_max, heightmap) = pack_foliage_heightmap(&cols, cols.len(), 256, 257);

        assert_eq!(y_min, 256);
        assert_eq!(y_max, 257);
        assert!(heightmap.is_none());
    }

    #[test]
    fn tile_y_ranges_none_when_no_foliage() {
        let cols = vec![u32::MAX; 64 * 64];
        assert!(compute_foliage_tile_y_ranges(&cols, 64, 64, 0).is_none());
    }

    #[test]
    fn tile_y_ranges_none_when_uniform() {
        // All columns at the same height → every tile has the same range → None.
        let cols = vec![100u32; 64 * 64];
        assert!(compute_foliage_tile_y_ranges(&cols, 64, 64, 100).is_none());
    }

    #[test]
    fn tile_y_ranges_some_when_different_heights() {
        // 64x64 grid with 8x8 tiles = 8x8 = 64 tiles.
        let mut cols = vec![u32::MAX; 64 * 64];
        // Tile (0,0): foliage at y=10
        cols[0] = 10;
        // Tile (4,4): foliage at y=200
        cols[32 + 32 * 64] = 200;

        let y_min = 10;
        let tiles = compute_foliage_tile_y_ranges(&cols, 64, 64, y_min)
            .expect("non-uniform tiles should produce Some");
        assert_eq!(tiles.len(), 64);
        // Tile (0,0): min_offset=0, max_offset=0 (surface y=10, y_min=10)
        assert_eq!(tiles[0] & 0xFFFF, 0);
        assert_eq!(tiles[0] >> 16, 0);
        // Tile (4,4): min_offset=190, max_offset=190 (surface y=200, y_min=10)
        assert_eq!(tiles[4 + 4 * 8] & 0xFFFF, 190);
        assert_eq!(tiles[4 + 4 * 8] >> 16, 190);
        // Empty tiles should be sentinel
        assert_eq!(tiles[1], 0xFFFF_FFFF);
        assert_eq!(tiles.iter().filter(|&&m| m != 0xFFFF_FFFF).count(), 2);
    }

    #[test]
    fn tile_y_ranges_min_max_within_tile() {
        // Two columns in the same tile at different heights.
        let mut cols = vec![u32::MAX; 64 * 64];
        cols[0] = 30; // y=30
        cols[1] = 50; // y=50

        let y_min = 30;
        let tiles = compute_foliage_tile_y_ranges(&cols, 64, 64, y_min)
            .expect("should produce per-tile data when tiles differ from empty");
        // Tile (0,0): min_offset=0 (30-30), max_offset=20 (50-30)
        assert_eq!(tiles[0] & 0xFFFF, 0);
        assert_eq!(tiles[0] >> 16, 20);
    }

    #[test]
    fn tile_y_ranges_full_chunk_256() {
        // 256x256 with 8x8 tiles -> 32x32 = 1024 tiles.
        let mut cols = vec![u32::MAX; 256 * 256];
        // Place foliage only in tile (31,31) at y=500.
        let x = 31 * TILE_SIZE;
        let z = 31 * TILE_SIZE;
        cols[(x + z * 256) as usize] = 500;

        let y_min = 500;
        let tiles = compute_foliage_tile_y_ranges(&cols, 256, 256, y_min)
            .expect("single tile with foliage, rest empty");
        assert_eq!(tiles.len(), 1024);
        // Only tile index 31 + 31*32 = 1023 should be non-sentinel.
        for (i, &m) in tiles.iter().enumerate() {
            if i == 1023 {
                // min_offset=0, max_offset=0 (only one column at y=500)
                assert_eq!(m & 0xFFFF, 0, "tile 1023 min_offset");
                assert_eq!(m >> 16, 0, "tile 1023 max_offset");
            } else {
                assert_eq!(m, 0xFFFF_FFFF, "tile {i} should be empty sentinel");
            }
        }
    }
}
