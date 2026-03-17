use bevy_ecs::resource::Resource;

#[derive(Resource, Clone)]
pub struct SharedVoxelBuffers {
    pub pool_buffer: wgpu::Buffer,
    pub avg_pool_buffer: wgpu::Buffer,
    pub indirection_buffer: wgpu::Buffer,
    pub streaming_info_buffer: wgpu::Buffer,
    pub render_settings_buffer: wgpu::Buffer,
}
