use bevy_ecs::resource::Resource;

/// GPU-facing data describing the prefab placement preview.
///
/// Updated each frame by the world editor; read by the renderer to upload
/// a preview-params uniform buffer consumed by the trace shader.
#[derive(Resource, Clone)]
pub struct PreviewGpuData {
    pub active: bool,
    pub position: [f32; 3],
    pub pool_offset: u32,
    pub world_size: u32,
    pub root_offset: u32,
    pub depth: u32,
    pub tint: [f32; 3],
    pub tint_strength: f32,
}

impl Default for PreviewGpuData {
    fn default() -> Self {
        Self {
            active: false,
            position: [0.0; 3],
            pool_offset: 0,
            world_size: 0,
            root_offset: 0,
            depth: 0,
            tint: [0.3, 0.6, 1.0],
            tint_strength: 0.35,
        }
    }
}
