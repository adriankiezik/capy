use capy_core::BakedChunkData;

use crate::dag::reduce_to_dag;
use crate::error::Result;
use crate::sparse64tree::{build_and_serialize_tree_with_heights, tree_to_gpu_data};
use crate::voxel_grid::VoxelGrid;

pub(crate) fn bake_chunk(grid: &VoxelGrid, col_heights: Option<&[u16]>) -> Result<BakedChunkData> {
    let flat = build_and_serialize_tree_with_heights(grid, col_heights);
    let dag = reduce_to_dag(&flat);
    let (info, dag_buffer, avg_color_buffer) = tree_to_gpu_data(&dag);

    Ok(BakedChunkData {
        dag_buffer,
        avg_color_buffer,
        root_offset: info.root_offset,
        world_size: info.world_size,
        depth: info.depth,
    })
}
