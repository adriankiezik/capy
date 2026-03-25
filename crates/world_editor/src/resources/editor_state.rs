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
    Smooth,
    Prefab,
    Select,
    /// Paint or remove foliage (grass) on surface voxels.
    Foliage,
    /// Place or remove water voxels.
    Water,
    /// Pick a voxel color from the world (ignores grass and water).
    ColorPick,
    /// Smart path creation: click waypoints, preview spline, confirm to apply.
    Path,
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
    /// Brush strength / opacity (0.0–1.0). Controls probability of each voxel
    /// being affected. 1.0 = all voxels, 0.5 = ~50% of voxels (spray effect).
    pub brush_strength: f32,
    /// Sculpt step size for Raise/Lower (1–64). Number of voxels to raise or
    /// lower per click.
    pub sculpt_step: u32,
    /// Smooth kernel radius (1–5). 1 = 3×3 neighborhood, 2 = 5×5, etc.
    pub smooth_kernel: u32,
    /// Number of smoothing iterations per click (1–10).
    pub smooth_iterations: u32,
    /// Color jitter amount (0.0–1.0). Randomly varies the painted color per
    /// voxel for organic-looking terrain.
    pub color_jitter: f32,
    /// Noise displacement for Place tool (0–32). Adds random vertical roughness
    /// to the brush surface.
    pub noise_displacement: u32,
    /// Foliage density (0.0–1.0). Probability of applying foliage per valid
    /// surface voxel.
    pub foliage_density: f32,
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
            brush_strength: 1.0,
            sculpt_step: 1,
            smooth_kernel: 1,
            smooth_iterations: 1,
            color_jitter: 0.0,
            noise_displacement: 0,
            foliage_density: 1.0,
        }
    }
}
