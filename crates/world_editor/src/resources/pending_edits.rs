use std::collections::HashMap;

use bevy_ecs::resource::Resource;
use capy_world::LeafBrickEdit;

/// Incremental brick edits accumulated since the last rebake frame.
/// Drained by the rebake system each frame.
#[derive(Resource, Default)]
pub struct PendingEdits {
    /// Changes grouped by chunk coord.
    pub by_chunk: HashMap<[i32; 3], Vec<LeafBrickEdit>>,
    /// When true, rebuild from canonical rather than patching incrementally.
    /// Set by undo/redo which replays all sparse edits.
    pub full_rebuild: bool,
}
