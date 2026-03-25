use bevy_ecs::system::{Res, ResMut};
use capy_core::{MATERIAL_COLORS, MouseButton, visual_material};

use crate::resources::{EditorState, EditorTool, InputEdge, VoxelHit};

pub(crate) fn color_pick(
    edge: Res<InputEdge>,
    voxel_hit: Option<Res<VoxelHit>>,
    mut state: ResMut<EditorState>,
) {
    if state.active_tool != EditorTool::ColorPick {
        return;
    }
    if !edge.mouse_just_pressed.contains(&MouseButton::Left) {
        return;
    }
    let Some(hit) = voxel_hit else {
        return;
    };
    if !hit.hit || hit.material == 0 {
        return;
    }

    let mat_id = visual_material(hit.material as u16);
    let color_f32 = MATERIAL_COLORS[mat_id as usize];
    state.picked_color = [
        (color_f32[0] * 255.0) as u8,
        (color_f32[1] * 255.0) as u8,
        (color_f32[2] * 255.0) as u8,
    ];
    state.selected_material = mat_id;
}
