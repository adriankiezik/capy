use bevy_ecs::resource::Resource;

#[derive(Resource)]
pub struct VoxelMeshData {
    pub dag_buffer: Vec<u32>,
    pub avg_color_buffer: Vec<u32>,
    pub world_size: u32,
    pub root_offset: u32,
    pub depth: u32,
    pub chunk_size: u32,
    pub material_palette: [[f32; 3]; 8],
}
