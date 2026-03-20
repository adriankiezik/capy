use capy_core::BakedChunkData;

use crate::dag::reduce_to_dag;
use crate::error::Result;
use crate::sparse64tree::{
    ChunkOccupancy, FlatTree, build_and_serialize_tree_with_heights, build_tree_from_mip,
    tree_to_gpu_data,
};
use crate::voxel_grid::VoxelGrid;

/// Full bake: builds occupancy mip from scratch + DAG reduction.
/// Used for initial world load and save.
pub fn bake_chunk(grid: &VoxelGrid, col_heights: Option<&[u16]>) -> Result<BakedChunkData> {
    let flat = build_and_serialize_tree_with_heights(grid, col_heights);
    let dag = reduce_to_dag(flat);
    let (info, dag_buffer, avg_color_buffer) = tree_to_gpu_data(dag);

    Ok(BakedChunkData {
        dag_buffer,
        avg_color_buffer,
        root_offset: info.root_offset,
        world_size: info.world_size,
        depth: info.depth,
    })
}

/// Fast bake: uses pre-built occupancy mip, skips DAG reduction.
/// Used for interactive editor updates where the mip is maintained incrementally.
pub fn bake_chunk_fast(grid: &VoxelGrid, occ: &ChunkOccupancy) -> Result<BakedChunkData> {
    let flat = build_tree_from_mip(grid, occ);
    let (info, dag_buffer, avg_color_buffer) = tree_to_gpu_data(flat);

    Ok(BakedChunkData {
        dag_buffer,
        avg_color_buffer,
        root_offset: info.root_offset,
        world_size: info.world_size,
        depth: info.depth,
    })
}

/// Compact a baked chunk by removing dead nodes and deduplicating identical subtrees.
/// This repacks the DAG buffer into a dense, cache-friendly layout.
pub fn compact_baked_chunk(baked: BakedChunkData) -> BakedChunkData {
    let flat = FlatTree {
        buffer: baked.dag_buffer,
        avg_color_buf: baked.avg_color_buffer,
        root_offset: baked.root_offset,
        world_size: baked.world_size,
        depth: baked.depth,
    };
    let dag = reduce_to_dag(flat);
    let (info, dag_buffer, avg_color_buffer) = tree_to_gpu_data(dag);
    BakedChunkData {
        dag_buffer,
        avg_color_buffer,
        root_offset: info.root_offset,
        world_size: info.world_size,
        depth: info.depth,
    }
}
