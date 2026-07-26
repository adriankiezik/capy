struct StreamingInfo {
    grid_min_x: i32,
    grid_min_y: i32,
    grid_min_z: i32,
    grid_dim_x: u32,
    grid_dim_y: u32,
    grid_dim_z: u32,
    chunk_size_xz: u32,
    chunk_size_y: u32,
};

struct SlotTreeInfo {
    world_size: u32,
    root_offset: u32,
    depth: u32,
    pool_offset: u32,
    foliage_y_min: u32,
    foliage_y_max: u32,
    foliage_bitmap_offset: u32,
    foliage_y_bands: u32,
    foliage_tile_y_ranges_offset: u32,
    solid_min_x: u32,
    solid_min_y: u32,
    solid_min_z: u32,
    solid_max_x: u32,
    solid_max_y: u32,
    solid_max_z: u32,
    flags: u32,
};

// Traversal stacks store only the node offset (pushed nodes are never leaves;
// masks are re-read from the pool on pop). Keeping the per-thread stack at
// 24 bytes instead of 96 reduces register/scratch pressure on the GPU.

struct HitResult {
    hit: bool,
    material: u32,
    normal: vec3<f32>,
    hit_pos_local: vec3<f32>,
    t: f32,              // ray parameter (world-space distance, since dir is normalized)
};

struct WaterHit {
    hit: bool,
    t: f32,                   // ray parameter at water surface (world-space distance)
    entry_normal: vec3<f32>,  // flat face normal from entry axis (before wave perturbation)
};
