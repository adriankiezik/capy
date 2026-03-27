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
};

struct StackEntry {
    node_idx: u32,
    mask_lo: u32,
    mask_hi: u32,
    is_leaf: bool,
};

struct HitResult {
    hit: bool,
    material: u32,
    normal: vec3<f32>,
    is_lod_hit: bool,
    color_override: vec3<f32>,
    lod_scale_exp: u32,
    hit_pos_local: vec3<f32>,
    t: f32,              // ray parameter (world-space distance, since dir is normalized)
};

struct WaterHit {
    hit: bool,
    t: f32,                   // ray parameter at water surface (world-space distance)
    entry_normal: vec3<f32>,  // flat face normal from entry axis (before wave perturbation)
};
