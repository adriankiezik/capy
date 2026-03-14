use bevy_ecs::resource::Resource;

#[derive(Resource)]
pub struct FrameTime {
    pub dt: f32,
}

impl Default for FrameTime {
    fn default() -> Self {
        Self { dt: 1.0 / 60.0 }
    }
}
