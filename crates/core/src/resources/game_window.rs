use std::sync::Arc;

use bevy_ecs::resource::Resource;

use crate::Window;

#[derive(Resource)]
pub struct GameWindow {
    pub handle: Arc<dyn Window>,
    pub width: u32,
    pub height: u32,
}
