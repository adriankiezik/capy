use bevy_ecs::resource::Resource;

/// Send+Sync handle to the GPU device and queue.
///
/// `GpuContext` is non-send because it holds a `wgpu::Surface` (which is
/// `!Send` on some platforms). This resource clones just the device and
/// queue (both internally Arc-backed) so that compute callbacks and other
/// systems can access the GPU without requiring non-send access.
#[derive(Resource, Clone)]
pub struct GpuAccess {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}
