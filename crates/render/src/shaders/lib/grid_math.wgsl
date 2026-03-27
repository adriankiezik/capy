fn world_to_chunk_coord(world_pos: vec3<f32>, cs_xz: f32, cs_y: f32) -> vec3<i32> {
    return vec3<i32>(
        i32(floor(world_pos.x / cs_xz)),
        i32(floor(world_pos.y / cs_y)),
        i32(floor(world_pos.z / cs_xz)),
    );
}

fn axis_normal(axis: i32, ray_dir: vec3<f32>) -> vec3<f32> {
    if axis == 0 {
        return vec3<f32>(select(1.0, -1.0, ray_dir.x >= 0.0), 0.0, 0.0);
    } else if axis == 1 {
        return vec3<f32>(0.0, select(1.0, -1.0, ray_dir.y >= 0.0), 0.0);
    } else if axis == 2 {
        return vec3<f32>(0.0, 0.0, select(1.0, -1.0, ray_dir.z >= 0.0));
    }
    return vec3<f32>(0.0, 1.0, 0.0);
}

fn get_cell_index(pos: vec3<f32>, scale_exp: u32) -> u32 {
    let ux = (bitcast<u32>(pos.x) >> scale_exp) & 3u;
    let uy = (bitcast<u32>(pos.y) >> scale_exp) & 3u;
    let uz = (bitcast<u32>(pos.z) >> scale_exp) & 3u;
    return ux + uy * 4u + uz * 16u;
}

fn floor_scale(pos: vec3<f32>, scale_exp: u32) -> vec3<f32> {
    let mask = ~0u << scale_exp;
    return vec3<f32>(
        bitcast<f32>(bitcast<u32>(pos.x) & mask),
        bitcast<f32>(bitcast<u32>(pos.y) & mask),
        bitcast<f32>(bitcast<u32>(pos.z) & mask),
    );
}

fn mirror_pos(pos: vec3<f32>, dir: vec3<f32>, range_check: bool) -> vec3<f32> {
    var mx = bitcast<f32>(bitcast<u32>(pos.x) ^ 0x7FFFFFu);
    var my = bitcast<f32>(bitcast<u32>(pos.y) ^ 0x7FFFFFu);
    var mz = bitcast<f32>(bitcast<u32>(pos.z) ^ 0x7FFFFFu);
    if range_check {
        if pos.x < 1.0 || pos.x >= 2.0 { mx = 3.0 - pos.x; }
        if pos.y < 1.0 || pos.y >= 2.0 { my = 3.0 - pos.y; }
        if pos.z < 1.0 || pos.z >= 2.0 { mz = 3.0 - pos.z; }
    }
    return vec3<f32>(
        select(pos.x, mx, dir.x > 0.0),
        select(pos.y, my, dir.y > 0.0),
        select(pos.z, mz, dir.z > 0.0),
    );
}

fn unmirror_pos(pos_m: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let ux = bitcast<f32>(bitcast<u32>(pos_m.x) ^ 0x7FFFFFu);
    let uy = bitcast<f32>(bitcast<u32>(pos_m.y) ^ 0x7FFFFFu);
    let uz = bitcast<f32>(bitcast<u32>(pos_m.z) ^ 0x7FFFFFu);
    return vec3<f32>(
        select(pos_m.x, ux, dir.x > 0.0),
        select(pos_m.y, uy, dir.y > 0.0),
        select(pos_m.z, uz, dir.z > 0.0),
    );
}

fn first_bit_high(x: u32) -> i32 {
    if x == 0u { return -1; }
    return i32(31u - countLeadingZeros(x));
}
