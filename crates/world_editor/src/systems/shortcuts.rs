use bevy_ecs::system::{Res, ResMut};
use capy_core::KeyCode;

use crate::resources::{BrushShape, EditorState, EditorTool, InputEdge};

pub(crate) fn shortcuts(edge: Res<InputEdge>, mut state: ResMut<EditorState>) {
    if edge.keys_just_pressed.contains(&KeyCode::Digit1) {
        state.active_tool = EditorTool::Place;
    }
    if edge.keys_just_pressed.contains(&KeyCode::Digit2) {
        state.active_tool = EditorTool::Remove;
    }
    if edge.keys_just_pressed.contains(&KeyCode::Digit3) {
        state.active_tool = EditorTool::Paint;
    }
    if edge.keys_just_pressed.contains(&KeyCode::Digit4) {
        state.active_tool = EditorTool::Raise;
    }
    if edge.keys_just_pressed.contains(&KeyCode::Digit5) {
        state.active_tool = EditorTool::Lower;
    }
    if edge.keys_just_pressed.contains(&KeyCode::Digit6) {
        state.active_tool = EditorTool::Flatten;
    }
    if edge.keys_just_pressed.contains(&KeyCode::Digit7) {
        state.active_tool = EditorTool::Smooth;
    }
    if edge.keys_just_pressed.contains(&KeyCode::Digit8) {
        state.active_tool = EditorTool::Prefab;
    }
    if edge.keys_just_pressed.contains(&KeyCode::Digit9) {
        state.active_tool = EditorTool::Select;
    }
    if edge.keys_just_pressed.contains(&KeyCode::KeyB) {
        state.brush_shape = match state.brush_shape {
            BrushShape::Sphere => BrushShape::Cube,
            BrushShape::Cube => BrushShape::Cylinder,
            BrushShape::Cylinder => BrushShape::Diamond,
            BrushShape::Diamond => BrushShape::Sphere,
        };
    }
    if edge.keys_just_pressed.contains(&KeyCode::BracketLeft) {
        state.brush_radius = state.brush_radius.saturating_sub(1).max(1);
    }
    if edge.keys_just_pressed.contains(&KeyCode::BracketRight) {
        state.brush_radius = (state.brush_radius + 1).min(128);
    }
    if state.active_tool == EditorTool::Prefab && edge.keys_just_pressed.contains(&KeyCode::KeyR) {
        state.prefab_rotation = (state.prefab_rotation + 1) % 4;
    }
}
