use bevy_ecs::resource::Resource;

#[derive(Resource, Clone)]
pub struct EguiContext(pub egui::Context);
