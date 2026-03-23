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
    /// Per-column foliage heightmap.
    /// `None` means all columns have foliage at the same height (canonical unedited chunk).
    /// Layout: a packed u16 offset per (x, z) column from `foliage_y_min`;
    /// `0xFFFF` means no foliage in that column.
    pub foliage_bitmap: Option<Vec<u32>>,
    /// 32-bit Y-occupancy mask: bit `i` is set when any foliage blade (root to tip)
    /// overlaps the 32-voxel band `[i*32, (i+1)*32)` in chunk-local Y space.
    /// Used by the shader to skip grass march steps in Y bands with no foliage.
    pub foliage_y_bands: u32,
    /// Per-tile (8×8 columns) packed min/max foliage surface Y offsets.
    /// Layout: one u32 per tile in row-major order (tile_x + tile_z * tiles_per_axis).
    /// Low 16 bits = min_surface_y offset from `foliage_y_min`.
    /// High 16 bits = max_surface_y offset from `foliage_y_min`.
    /// `0xFFFFFFFF` = empty tile (no foliage).
    /// `None` when there is no foliage or when all tiles have the same range.
    pub foliage_tile_y_ranges: Option<Vec<u32>>,
}
