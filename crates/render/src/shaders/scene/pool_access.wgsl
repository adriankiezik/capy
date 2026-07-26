// 16 words = 64 bytes: one slot per GPU cache line (word 15 stores flags).
const INDIRECTION_STRIDE: u32 = 16u;

fn lookup_chunk_info(cc: vec3<i32>) -> SlotTreeInfo {
    var info: SlotTreeInfo;
    info.world_size = 0u;
    info.flags = 0u;

    let lx = cc.x - streaming.grid_min_x;
    let ly = cc.y - streaming.grid_min_y;
    let lz = cc.z - streaming.grid_min_z;
    if lx < 0 || ly < 0 || lz < 0
        || lx >= i32(streaming.grid_dim_x)
        || ly >= i32(streaming.grid_dim_y)
        || lz >= i32(streaming.grid_dim_z) {
        return info;
    }
    let idx = u32(lx) + u32(ly) * streaming.grid_dim_x + u32(lz) * streaming.grid_dim_x * streaming.grid_dim_y;
    let base = idx * INDIRECTION_STRIDE;
    info.world_size = indirection[base];
    // Empty slot: skip the remaining 14 loads (common case while chunk-marching).
    if info.world_size == 0u {
        return info;
    }
    info.root_offset = indirection[base + 1u];
    info.depth = indirection[base + 2u];
    info.pool_offset = indirection[base + 3u];
    info.foliage_y_min = indirection[base + 4u];
    info.foliage_y_max = indirection[base + 5u];
    info.foliage_bitmap_offset = indirection[base + 6u];
    info.foliage_y_bands = indirection[base + 7u];
    info.foliage_tile_y_ranges_offset = indirection[base + 8u];
    info.solid_min_x = indirection[base + 9u];
    info.solid_min_y = indirection[base + 10u];
    info.solid_min_z = indirection[base + 11u];
    info.solid_max_x = indirection[base + 12u];
    info.solid_max_y = indirection[base + 13u];
    info.solid_max_z = indirection[base + 14u];
    info.flags = indirection[base + 15u];
    return info;
}

fn slot_solid_aabb_valid(info: SlotTreeInfo) -> bool {
    return info.solid_min_x < info.solid_max_x
        && info.solid_min_y < info.solid_max_y
        && info.solid_min_z < info.solid_max_z;
}

fn slot_solid_aabb_min(info: SlotTreeInfo) -> vec3<f32> {
    return vec3<f32>(
        f32(info.solid_min_x),
        f32(info.solid_min_y),
        f32(info.solid_min_z),
    );
}

fn slot_solid_aabb_max(info: SlotTreeInfo) -> vec3<f32> {
    return vec3<f32>(
        f32(info.solid_max_x),
        f32(info.solid_max_y),
        f32(info.solid_max_z),
    );
}

fn slot_solid_aabb_entry_axis(info: SlotTreeInfo, local_origin: vec3<f32>, dir: vec3<f32>) -> i32 {
    let inv_dir = 1.0 / dir;
    let t1 = (slot_solid_aabb_min(info) - local_origin) * inv_dir;
    let t2 = (slot_solid_aabb_max(info) - local_origin) * inv_dir;
    let t_min_v = min(t1, t2);
    if t_min_v.x >= t_min_v.y && t_min_v.x >= t_min_v.z {
        return 0;
    }
    if t_min_v.y >= t_min_v.z {
        return 1;
    }
    return 2;
}

fn pool_read(pool_base: u32, offset: u32) -> u32 {
    return chunk_pool[pool_base + offset];
}

fn pool_read_avg(pool_base: u32, offset: u32) -> u32 {
    return chunk_avg_pool[pool_base + offset];
}

fn get_child_offset_pool(pool_base: u32, node_offset: u32, child_packed_idx: u32) -> u32 {
    return pool_read(pool_base, node_offset + 3u + child_packed_idx);
}

fn get_leaf_material_pool(pool_base: u32, node_offset: u32, bit: u32) -> u32 {
    let word_idx = bit / 2u;
    let half_off = bit % 2u;
    let word = pool_read(pool_base, node_offset + 3u + word_idx);
    return (word >> (half_off * 16u)) & 0xFFFFu;
}

const NODE_FLAG_LEAF: u32 = 0x1u;
const NODE_FLAG_UNIFORM_WATER: u32 = 0x2u;
const WATER_BIT_MASK: u32 = 0x4000u;

// Child-pointer tag bits, mirrored from the child header when the CPU
// assembles chunk DAGs into the pool (capy_core::tag_pool_child_pointers).
// They let traversal classify a child without reading its header first,
// removing one dependent memory load per descent level.
const CHILD_PTR_FLAG_LEAF: u32 = 0x80000000u;
const CHILD_PTR_FLAG_UNIFORM_WATER: u32 = 0x40000000u;
const CHILD_PTR_OFFSET_MASK: u32 = 0x3FFFFFFFu;

fn child_ptr_offset(child_ptr: u32) -> u32 {
    return child_ptr & CHILD_PTR_OFFSET_MASK;
}

fn child_ptr_is_leaf(child_ptr: u32) -> bool {
    return (child_ptr & CHILD_PTR_FLAG_LEAF) != 0u;
}

fn child_ptr_is_uniform_water(child_ptr: u32) -> bool {
    return (child_ptr & CHILD_PTR_FLAG_UNIFORM_WATER) != 0u;
}

fn get_node_flags_pool(pool_base: u32, node_offset: u32) -> u32 {
    return pool_read(pool_base, node_offset + 2u);
}

fn node_is_leaf(flags: u32) -> bool {
    return (flags & NODE_FLAG_LEAF) != 0u;
}

fn node_is_uniform_water(flags: u32) -> bool {
    return (flags & NODE_FLAG_UNIFORM_WATER) != 0u;
}

fn get_node_avg_color_pool(pool_base: u32, node_offset: u32) -> vec3<f32> {
    let packed = pool_read_avg(pool_base, node_offset);
    let r = f32(packed & 0xFFu) / 255.0;
    let g = f32((packed >> 8u) & 0xFFu) / 255.0;
    let b = f32((packed >> 16u) & 0xFFu) / 255.0;
    return vec3<f32>(r, g, b);
}
