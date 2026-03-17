use bevy_ecs::resource::Resource;
use capy_core::MaterialId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTool {
    Place,
    Remove,
    Paint,
    Raise,
    Lower,
    Flatten,
    Smooth,
    Prefab,
    Select,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushShape {
    Sphere,
    Cube,
}

#[derive(Resource)]
pub struct EditorState {
    pub active_tool: EditorTool,
    pub brush_radius: u32,
    pub brush_shape: BrushShape,
    pub selected_material: MaterialId,
    /// Color chosen via the egui color picker (sRGB, 0-255).
    pub picked_color: [u8; 3],
}

impl Default for EditorState {
    fn default() -> Self {
        let picked_color = [204u8, 51, 51];
        let color_f32 = [
            picked_color[0] as f32 / 255.0,
            picked_color[1] as f32 / 255.0,
            picked_color[2] as f32 / 255.0,
        ];
        Self {
            active_tool: EditorTool::Place,
            brush_radius: 1,
            brush_shape: BrushShape::Sphere,
            selected_material: capy_core::closest_material(color_f32),
            picked_color,
        }
    }
}
