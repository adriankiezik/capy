use bevy_ecs::error::BevyError;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

pub type RenderOverlayCallback = fn(
    world: &mut World,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    encoder: &mut wgpu::CommandEncoder,
    output_view: &wgpu::TextureView,
) -> Result<(), BevyError>;

#[derive(Resource, Default)]
pub struct RenderOverlayCallbacks {
    callbacks: Vec<RenderOverlayCallback>,
}

impl RenderOverlayCallbacks {
    pub fn register_callback(world: &mut World, callback: RenderOverlayCallback) {
        let mut callbacks = world.get_resource_or_init::<RenderOverlayCallbacks>();
        callbacks.register(callback);
    }

    pub(crate) fn register(&mut self, callback: RenderOverlayCallback) {
        if self
            .callbacks
            .iter()
            .any(|registered| std::ptr::fn_addr_eq(*registered, callback))
        {
            return;
        }
        self.callbacks.push(callback);
    }

    pub(crate) fn list(&self) -> &[RenderOverlayCallback] {
        &self.callbacks
    }
}
