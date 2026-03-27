
fn intersect_aabb(
    origin: vec3<f32>,
    dir: vec3<f32>,
    aabb_min: vec3<f32>,
    aabb_max: vec3<f32>,
) -> vec2<f32> {
    let inv_dir = 1.0 / dir;
    let t1 = (aabb_min - origin) * inv_dir;
    let t2 = (aabb_max - origin) * inv_dir;
    let t_min_v = min(t1, t2);
    let t_max_v = max(t1, t2);
    let t_enter = max(max(t_min_v.x, t_min_v.y), t_min_v.z);
    let t_exit  = min(min(t_max_v.x, t_max_v.y), t_max_v.z);
    return vec2<f32>(t_enter, t_exit);
}
