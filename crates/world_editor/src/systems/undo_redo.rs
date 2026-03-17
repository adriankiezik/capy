use bevy_ecs::system::{NonSend, Res, ResMut};
use capy_core::{KeyCode, RawInput};
use capy_world::LeafBrickEdit;

use crate::resources::{
    EditAction, EditHistory, EditTask, EditableWorld, InputEdge, MeshDirty, PendingEdits,
};

pub(crate) fn undo_redo(
    input: Res<RawInput>,
    edge: Res<InputEdge>,
    edit_task: Option<NonSend<EditTask>>,
    mut editable: ResMut<EditableWorld>,
    mut history: ResMut<EditHistory>,
    mut dirty: ResMut<MeshDirty>,
    mut pending: ResMut<PendingEdits>,
) {
    if edit_task
        .as_ref()
        .is_some_and(|task| task.pending.as_ref().is_some())
    {
        return;
    }

    let ctrl_held = input.keys_held.contains(&KeyCode::ControlLeft)
        || input.keys_held.contains(&KeyCode::ControlRight);

    if !ctrl_held {
        return;
    }

    // Undo: Ctrl+Z
    if edge.keys_just_pressed.contains(&KeyCode::KeyZ)
        && let Some(action) = history.undo_stack.pop()
    {
        apply_action(&mut editable, &mut dirty, &mut pending, &action, true);
        history.redo_stack.push(action);
    }

    // Redo: Ctrl+Y
    if edge.keys_just_pressed.contains(&KeyCode::KeyY)
        && let Some(action) = history.redo_stack.pop()
    {
        apply_action(&mut editable, &mut dirty, &mut pending, &action, false);
        history.undo_stack.push(action);
    }
}

fn apply_action(
    editable: &mut EditableWorld,
    dirty: &mut MeshDirty,
    pending: &mut PendingEdits,
    action: &EditAction,
    undo: bool,
) {
    let mut affected_chunks = std::collections::HashSet::new();

    for change in &action.changes {
        let materials = if undo {
            change.old_materials
        } else {
            change.new_materials
        };

        let chunk = editable.chunks.entry(change.chunk).or_default();
        chunk.write_brick(change.brick[0], change.brick[1], change.brick[2], materials);

        affected_chunks.insert(change.chunk);
    }

    // For undo/redo, we need a full rebuild from canonical for affected chunks.
    // Push ALL sparse brick edits for each affected chunk into PendingEdits.
    pending.full_rebuild = true;
    for cc in &affected_chunks {
        dirty.dirty.insert(*cc);

        let brick_edits: Vec<LeafBrickEdit> = if let Some(chunk) = editable.chunks.get(cc) {
            chunk
                .bricks
                .iter()
                .map(|(&coord, &materials)| LeafBrickEdit {
                    bx: coord[0],
                    by: coord[1],
                    bz: coord[2],
                    materials,
                })
                .collect()
        } else {
            Vec::new()
        };
        pending.by_chunk.insert(*cc, brick_edits);
    }
}
