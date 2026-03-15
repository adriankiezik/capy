use bevy_ecs::error::BevyError;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

pub type ComputePassEncode = fn(
    world: &mut World,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), BevyError>;

pub type ComputePassPostSubmit = fn(world: &mut World) -> Result<(), BevyError>;

#[derive(Clone, Copy)]
pub struct ComputePassCallback {
    pub encode: ComputePassEncode,
    pub post_submit: Option<ComputePassPostSubmit>,
}

#[derive(Resource, Default)]
pub struct ComputePassCallbacks {
    callbacks: Vec<ComputePassCallback>,
}

impl ComputePassCallbacks {
    pub fn register_callback(
        world: &mut World,
        encode: ComputePassEncode,
        post_submit: Option<ComputePassPostSubmit>,
    ) {
        let mut callbacks = world.get_resource_or_init::<ComputePassCallbacks>();
        callbacks.register(encode, post_submit);
    }

    pub(crate) fn register(
        &mut self,
        encode: ComputePassEncode,
        post_submit: Option<ComputePassPostSubmit>,
    ) {
        if self
            .callbacks
            .iter()
            .any(|registered| std::ptr::fn_addr_eq(registered.encode, encode))
        {
            return;
        }
        self.callbacks.push(ComputePassCallback {
            encode,
            post_submit,
        });
    }

    pub(crate) fn list(&self) -> &[ComputePassCallback] {
        &self.callbacks
    }
}
