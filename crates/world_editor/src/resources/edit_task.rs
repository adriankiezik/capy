use std::sync::mpsc::Receiver;

use capy_world::LeafBrickEdit;

use super::edit_history::BrickChange;
use super::editable_world::EditableChunk;

pub(crate) struct UpdatedChunk {
    pub(crate) coord: [i32; 3],
    pub(crate) chunk: EditableChunk,
    pub(crate) pending: Vec<LeafBrickEdit>,
}

pub(crate) struct EditTaskOutput {
    pub(crate) updated_chunks: Vec<UpdatedChunk>,
    pub(crate) changes: Vec<BrickChange>,
    pub(crate) loop_ms: f64,
    pub(crate) worker_ms: f64,
    pub(crate) radius: i32,
}

#[derive(Default)]
pub struct EditTask {
    pub(crate) pending: Option<Receiver<EditTaskOutput>>,
}
