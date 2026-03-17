use bevy_ecs::resource::Resource;
use capy_core::MaterialId;

pub struct BrickChange {
    pub chunk: [i32; 3],
    pub brick: [u32; 3],
    pub old_materials: [MaterialId; 64],
    pub new_materials: [MaterialId; 64],
}

pub struct EditAction {
    pub changes: Vec<BrickChange>,
}

#[derive(Resource, Default)]
pub struct EditHistory {
    pub undo_stack: Vec<EditAction>,
    pub redo_stack: Vec<EditAction>,
}
