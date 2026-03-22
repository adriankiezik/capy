#[derive(Debug, Clone)]
pub struct BakedChunkData {
    pub dag_buffer: Vec<u32>,
    pub avg_color_buffer: Vec<u32>,
    pub root_offset: u32,
    pub world_size: u32,
    pub depth: u32,
    /// Lowest chunk-local Y voxel with a foliage material (inclusive).
    /// When `foliage_y_min >= foliage_y_max`, the chunk has no foliage.
    pub foliage_y_min: u32,
    /// Highest chunk-local Y voxel with a foliage material (exclusive).
    pub foliage_y_max: u32,
    /// Per-column foliage bitmap: 1 bit per (x, z) column, `chunk_size_xz × chunk_size_xz` bits.
    /// `None` means all columns have foliage (canonical unedited chunk).
    /// Layout: bit index = x + z * chunk_size_xz, packed into u32 words.
    pub foliage_bitmap: Option<Vec<u32>>,
    /// 32-bit Y-occupancy mask: bit `i` is set when any foliage blade (root to tip)
    /// overlaps the 32-voxel band `[i*32, (i+1)*32)` in chunk-local Y space.
    /// Used by the shader to skip grass march steps in Y bands with no foliage.
    pub foliage_y_bands: u32,
}
