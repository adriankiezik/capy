use std::collections::HashMap;

use bevy_ecs::resource::Resource;
use capy_core::{BakedChunkData, NearVoxelMeshData};

#[derive(Resource)]
pub(crate) struct WorldGrid {
    pub canonical_baked: BakedChunkData,
    /// Disposable baked-cache for edited chunks. The source of truth is `EditableWorld`.
    pub edited_baked: HashMap<[i32; 3], BakedChunkData>,
    /// Per-edited-chunk meshes; unchanged entries survive incremental rebakes.
    pub near_mesh_cache: HashMap<[i32; 3], NearVoxelMeshData>,
    /// Local-space mesh shared by every unedited canonical chunk slot.
    pub canonical_near_mesh: NearVoxelMeshData,
    pub grid_dim_xz: u32,
}
