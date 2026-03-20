use bevy_ecs::resource::Resource;

#[derive(Resource)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: String::from("Capy"),
            width: 1280,
            height: 720,
            vsync: false,
        }
    }
}
