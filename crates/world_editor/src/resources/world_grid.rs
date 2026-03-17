use std::collections::HashMap;

use bevy_ecs::resource::Resource;
use capy_core::BakedChunkData;

#[derive(Resource)]
pub(crate) struct WorldGrid {
    pub canonical_baked: BakedChunkData,
    /// Disposable baked-cache for edited chunks. The source of truth is `EditableWorld`.
    pub edited_baked: HashMap<[i32; 3], BakedChunkData>,
    pub grid_dim_xz: u32,
}
