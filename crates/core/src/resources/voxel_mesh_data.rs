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

impl VoxelMeshData {
    pub fn empty() -> Self {
        Self {
            dag_buffer: vec![0],
            avg_color_buffer: vec![0],
            world_size: 0,
            root_offset: 0,
            depth: 0,
            chunk_size: 1,
            material_palette: [[0.0; 3]; 8],
        }
    }
}
