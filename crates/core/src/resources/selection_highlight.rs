use bevy_ecs::resource::Resource;

/// GPU-facing selection highlight data.
///
/// Updated each frame by the world editor; read by the renderer to tint
/// voxels inside the selection AABB.
#[derive(Resource, Clone, Default)]
pub struct SelectionHighlight {
    pub active: bool,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
}
