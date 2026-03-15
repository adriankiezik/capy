use bevy_ecs::resource::Resource;

#[derive(Resource, Default)]
pub struct CursorPosition {
    pub x: f32,
    pub y: f32,
}
