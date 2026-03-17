use std::collections::HashSet;

use bevy_ecs::resource::Resource;

#[derive(Resource, Default)]
pub struct MeshDirty {
    pub dirty: HashSet<[i32; 3]>,
}
