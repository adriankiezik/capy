use bevy_ecs::resource::Resource;

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum CursorMode {
    #[default]
    Free,
    Confined,
}
