use bevy_ecs::resource::Resource;

#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct RenderResolution {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl RenderResolution {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}
