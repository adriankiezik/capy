use bevy_ecs::resource::Resource;
use capy_core::BakedChunkData;

/// Cached baked DAG data for the currently selected prefab preview.
///
/// Populated by `prefab_preview_bake` when the selected prefab changes;
/// consumed by the rebake system to append preview data to the pool buffers.
#[derive(Resource, Default)]
pub struct PreviewBake {
    /// Baked chunk data for the selected prefab, or `None` if no prefab is
    /// selected / prefab hasn't been baked yet.
    pub baked: Option<BakedChunkData>,
    /// Source path of the prefab that was baked, used to detect changes.
    pub source_path: Option<std::path::PathBuf>,
    /// Set when baked data changes and needs to be appended to the pool.
    pub needs_pool_rebuild: bool,
}
