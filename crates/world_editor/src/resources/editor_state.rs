use std::time::Instant;

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
    /// Paint or remove foliage (grass) on surface voxels.
    Foliage,
    /// Place or remove water voxels.
    Water,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoliageAction {
    /// Paint foliage material onto surface voxels.
    Paint,
    /// Remove foliage material from surface voxels (replace with dirt).
    Erase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoliageMode {
    /// Only affect voxels at the exact Y level of the ray hit.
    SingleLevel,
    /// Affect all surface voxels (air above) within the brush, at any Y level.
    Surface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterAction {
    /// Place water voxels in empty space.
    Place,
    /// Remove water voxels.
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushShape {
    Sphere,
    Cube,
    Cylinder,
    Diamond,
}

#[derive(Resource)]
pub struct EditorState {
    pub active_tool: EditorTool,
    pub brush_radius: u32,
    pub brush_shape: BrushShape,
    pub selected_material: MaterialId,
    /// Color chosen via the egui color picker (sRGB, 0-255).
    pub picked_color: [u8; 3],
    /// Search filter for the prefab list.
    pub prefab_search: String,
    /// Timestamp of the last scroll event for prefab resolution; used to
    /// throttle regeneration until scrolling settles.
    pub prefab_scroll_last: Option<Instant>,
    /// Prefab rotation in 90° increments (0..4) around the Y axis.
    pub prefab_rotation: u8,
    /// Current foliage tool action (paint or erase).
    pub foliage_action: FoliageAction,
    /// Foliage brush mode: single Y level or all surface voxels.
    pub foliage_mode: FoliageMode,
    /// Current water tool action (place or remove).
    pub water_action: WaterAction,
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
            prefab_search: String::new(),
            prefab_scroll_last: None,
            prefab_rotation: 0,
            foliage_action: FoliageAction::Paint,
            foliage_mode: FoliageMode::SingleLevel,
            water_action: WaterAction::Place,
        }
    }
}
