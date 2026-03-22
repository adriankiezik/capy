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
/// Returns (y_min, y_max, heightmap) where the heightmap stores a per-column u8
/// offset from `y_min` (0xFF = no foliage), packed 4 per u32.
/// `None` means all columns have foliage at the same height.
fn scan_foliage(grid: &VoxelGrid) -> (u32, u32, Option<Vec<u32>>, u32) {
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
        return (0, 0, None, 0);
    }

    let bands = compute_foliage_y_bands(&col_surface_y);
    let (ym, yx, hm) = pack_foliage_heightmap(&col_surface_y, num_cols, y_min, y_max);
    (ym, yx, hm, bands)
}

/// Full bake: builds occupancy mip from scratch + DAG reduction.
/// Used for initial world load and save.
pub fn bake_chunk(grid: &VoxelGrid, col_heights: Option<&[u16]>) -> Result<BakedChunkData> {
    let (foliage_y_min, foliage_y_max, foliage_bitmap, foliage_y_bands) = scan_foliage(grid);
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
    })
}

/// Fast bake: uses pre-built occupancy mip, skips DAG reduction.
/// Used for interactive editor updates where the mip is maintained incrementally.
pub fn bake_chunk_fast(grid: &VoxelGrid, occ: &ChunkOccupancy) -> Result<BakedChunkData> {
    let (foliage_y_min, foliage_y_max, foliage_bitmap, foliage_y_bands) = scan_foliage(grid);
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
        return;
    }

    let bands = compute_foliage_y_bands(&col_surface_y);
    let (new_y_min, new_y_max, heightmap) =
        pack_foliage_heightmap(&col_surface_y, num_cols, y_min, y_max);
    baked.foliage_y_min = new_y_min;
    baked.foliage_y_max = new_y_max;
    baked.foliage_bitmap = heightmap;
    baked.foliage_y_bands = bands;
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

/// Pack per-column foliage surface Y values into a u8 heightmap (4 bytes per u32).
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

    // Pack per-column u8 offsets: surface_y - y_min, or 0xFF for no foliage.
    let heightmap_words = (num_cols + 3) / 4;
    let mut heightmap = vec![0xFFFFFFFFu32; heightmap_words]; // all 0xFF = no foliage

    for col in 0..num_cols {
        if col_surface_y[col] != u32::MAX {
            let offset = (col_surface_y[col] - y_min).min(254) as u8;
            let word_idx = col / 4;
            let byte_shift = (col % 4) * 8;
            // Clear the byte slot (was 0xFF) and set the offset.
            heightmap[word_idx] &= !(0xFF << byte_shift);
            heightmap[word_idx] |= (offset as u32) << byte_shift;
        }
    }

    (y_min, y_max, Some(heightmap))
}

const BRANCH: u32 = 4;
