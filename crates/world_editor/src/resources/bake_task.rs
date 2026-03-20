use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use capy_core::{BakedChunkData, VoxelMeshData};

pub(crate) struct RebakeOutput {
    pub(crate) edited_baked: HashMap<[i32; 3], BakedChunkData>,
    pub(crate) mesh: VoxelMeshData,
    pub(crate) upload: capy_render::PreparedVoxelSceneUpload,
    pub(crate) num_chunks: usize,
    pub(crate) rebuilt_chunks: usize,
    pub(crate) total_bricks: usize,
    pub(crate) patch_ms: f64,
    pub(crate) mesh_ms: f64,
    /// Pool offset of appended preview DAG, if any.
    pub(crate) preview_pool_offset: Option<u32>,
}

#[derive(Default)]
pub struct BakeTask {
    pub(crate) pending: Option<Receiver<anyhow::Result<RebakeOutput>>>,
}
