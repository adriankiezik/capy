use bevy_ecs::system::{NonSend, Res, ResMut};
use capy_core::{KeyCode, RawInput};
use capy_world::LeafBrickEdit;

use crate::resources::{
    BrickChange, EditAction, EditHistory, EditTask, EditableWorld, InputEdge, MeshDirty,
    PendingEdits, SelectionChange, SelectionPhase, SelectionState,
};

pub(crate) fn undo_redo(
    input: Res<RawInput>,
    edge: Res<InputEdge>,
    edit_task: Option<NonSend<EditTask>>,
    mut editable: ResMut<EditableWorld>,
    mut history: ResMut<EditHistory>,
    mut dirty: ResMut<MeshDirty>,
    mut pending: ResMut<PendingEdits>,
    mut sel: ResMut<SelectionState>,
) {
    if edit_task
        .as_ref()
        .is_some_and(|task| task.pending.as_ref().is_some())
    {
        return;
    }

    let ctrl_held = input.keys_held.contains(&KeyCode::ControlLeft)
        || input.keys_held.contains(&KeyCode::ControlRight);

    let z_just = ctrl_held && edge.keys_just_pressed.contains(&KeyCode::KeyZ);
    let z_held = ctrl_held && input.keys_held.contains(&KeyCode::KeyZ);
    let y_just = ctrl_held && edge.keys_just_pressed.contains(&KeyCode::KeyY);
    let y_held = ctrl_held && input.keys_held.contains(&KeyCode::KeyY);

    // Undo: Ctrl+Z (with hold-to-repeat)
    if history.undo_repeat.update(z_just, z_held) {
        if let Some(action) = history.undo_stack.pop() {
            apply_action(
                &mut editable,
                &mut dirty,
                &mut pending,
                &mut sel,
                &action,
                true,
            );
            history.redo_stack.push(action);
        }
    }

    // Redo: Ctrl+Y (with hold-to-repeat)
    if history.redo_repeat.update(y_just, y_held) {
        if let Some(action) = history.redo_stack.pop() {
            apply_action(
                &mut editable,
                &mut dirty,
                &mut pending,
                &mut sel,
                &action,
                false,
            );
            history.undo_stack.push(action);
        }
    }
}

fn apply_action(
    editable: &mut EditableWorld,
    dirty: &mut MeshDirty,
    pending: &mut PendingEdits,
    sel: &mut SelectionState,
    action: &EditAction,
    undo: bool,
) {
    match action {
        EditAction::Voxel { changes } => {
            apply_voxel_changes(editable, dirty, pending, changes, undo);
        }
        EditAction::Selection(change) => {
            apply_selection_change(sel, change, undo);
        }
        EditAction::VoxelMove { changes, selection } => {
            apply_voxel_changes(editable, dirty, pending, changes, undo);
            apply_selection_change(sel, selection, undo);
        }
    }
}

fn apply_voxel_changes(
    editable: &mut EditableWorld,
    dirty: &mut MeshDirty,
    pending: &mut PendingEdits,
    changes: &[BrickChange],
    undo: bool,
) {
    let mut affected_chunks = std::collections::HashSet::new();

    for change in changes {
        let materials = if undo {
            change.old_materials
        } else {
            change.new_materials
        };

        let chunk = editable.chunks.entry(change.chunk).or_default();
        chunk.write_brick(change.brick[0], change.brick[1], change.brick[2], materials);

        affected_chunks.insert(change.chunk);
    }

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

fn apply_selection_change(sel: &mut SelectionState, change: &SelectionChange, undo: bool) {
    let (a, b) = if undo {
        (change.old_a, change.old_b)
    } else {
        (change.new_a, change.new_b)
    };
    sel.corner_a = a;
    sel.corner_b = b;
    if a.is_some() && b.is_some() {
        sel.phase = SelectionPhase::Selected;
    } else {
        sel.clear();
    }
}
