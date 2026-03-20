@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> streaming: StreamingInfo;
@group(0) @binding(2) var<storage, read> chunk_pool: array<u32>;
@group(0) @binding(3) var<storage, read> chunk_avg_pool: array<u32>;
@group(0) @binding(4) var<storage, read> indirection: array<u32>;
@group(0) @binding(5) var<uniform> render_settings: RenderSettingsUniform;

@group(0) @binding(6) var gbuf_depth: texture_2d<f32>;
@group(0) @binding(7) var gbuf_normal: texture_2d<f32>;
@group(0) @binding(8) var ao_output: texture_storage_2d<r32float, write>;

struct RtaoParams {
    ao_radius: f32,
    ao_intensity: f32,
    ao_rays: u32,
    frame_index: u32,
};
@group(0) @binding(9) var<uniform> rtao_params: RtaoParams;

const PI: f32 = 3.14159265359;

fn get_ray_dir(pixel: vec2<f32>, resolution: vec2<f32>) -> vec3<f32> {
    let uv_x = (pixel.x + 0.5 + camera.jitter.x) / resolution.x;
    let uv_y = 1.0 - (pixel.y + 0.5 + camera.jitter.y) / resolution.y;
    return normalize(
        camera.ray_corner
        + camera.ray_right * (uv_x * 2.0)
        + camera.ray_up * (uv_y * 2.0)
    );
}

fn hash_u32(x: u32) -> u32 {
    var v = x;
    v ^= v >> 16u;
    v *= 0x45d9f3bu;
    v ^= v >> 16u;
    v *= 0x45d9f3bu;
    v ^= v >> 16u;
    return v;
}

fn hash_to_float(h: u32) -> f32 {
    return f32(h & 0xFFFFFFu) / f32(0xFFFFFFu);
}

fn build_onb(n: vec3<f32>) -> mat3x3<f32> {
    var t: vec3<f32>;
    if abs(n.y) < 0.999 {
        t = normalize(cross(n, vec3<f32>(0.0, 1.0, 0.0)));
    } else {
        t = normalize(cross(n, vec3<f32>(1.0, 0.0, 0.0)));
    }
    let b = cross(n, t);
    return mat3x3<f32>(t, b, n);
}

fn cosine_hemisphere(u1: f32, u2: f32) -> vec3<f32> {
    let r = sqrt(u1);
    let phi = 2.0 * PI * u2;
    return vec3<f32>(r * cos(phi), r * sin(phi), sqrt(max(1.0 - u1, 0.0)));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(gbuf_depth);
    let x = gid.x;
    let y = gid.y;
    if x >= dims.x || y >= dims.y { return; }

    let pixel = vec2<i32>(i32(x), i32(y));
    let resolution = vec2<f32>(camera.resolution);

    let normal_sample = textureLoad(gbuf_normal, pixel, 0);
    if normal_sample.w <= 0.0 {
        textureStore(ao_output, pixel, vec4<f32>(1.0, 0.0, 0.0, 0.0));
        return;
    }

    let center_depth = textureLoad(gbuf_depth, pixel, 0).r;
    if center_depth <= 0.0 {
        textureStore(ao_output, pixel, vec4<f32>(1.0, 0.0, 0.0, 0.0));
        return;
    }

    let normal = normalize(normal_sample.xyz);
    let view_dir = get_ray_dir(vec2<f32>(pixel), resolution);
    let world_pos = camera.camera_pos + view_dir * center_depth;

    let onb = build_onb(normal);
    let ao_origin = world_pos + normal * max(render_settings.ray_epsilon, 0.001);
    let radius = rtao_params.ao_radius;
    let num_rays = rtao_params.ao_rays;

    let pixel_idx = y * dims.x + x;
    var seed = hash_u32(rtao_params.frame_index * (dims.x * dims.y) + pixel_idx);

    var occlusion = 0.0;

    for (var i = 0u; i < num_rays; i++) {
        seed = hash_u32(seed + i * 0x9E3779B9u);
        let u1 = hash_to_float(seed);
        seed = hash_u32(seed);
        let u2 = hash_to_float(seed);

        let local_dir = cosine_hemisphere(u1, u2);
        let world_dir = normalize(onb * local_dir);

        if trace_ao_ray(ao_origin, world_dir, radius) {
            occlusion += 1.0;
        }
    }

    let visibility = 1.0 - occlusion / f32(num_rays);
    let ao = clamp(pow(visibility, rtao_params.ao_intensity), 0.0, 1.0);

    textureStore(ao_output, pixel, vec4<f32>(ao, 0.0, 0.0, 0.0));
}
