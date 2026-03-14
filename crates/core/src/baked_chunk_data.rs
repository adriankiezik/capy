#[derive(Debug, Clone)]
pub struct BakedChunkData {
    pub dag_buffer: Vec<u32>,
    pub avg_color_buffer: Vec<u32>,
    pub root_offset: u32,
    pub world_size: u32,
    pub depth: u32,
}
