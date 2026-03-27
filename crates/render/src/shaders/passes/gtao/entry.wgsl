struct GtaoParams {
    ao_radius: f32,
    ao_intensity: f32,
    ao_samples: u32,
    ao_steps: u32,
};

@group(0) @binding(0) var gbuf_depth: texture_2d<f32>;
@group(0) @binding(1) var gbuf_normal: texture_2d<f32>;
@group(0) @binding(2) var ao_output: texture_storage_2d<r32float, write>;
@group(0) @binding(3) var<uniform> camera: CameraUniform;
@group(0) @binding(4) var<uniform> gtao_params: GtaoParams;

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

fn reconstruct_position(pixel: vec2<i32>, resolution: vec2<f32>) -> vec3<f32> {
    let depth = textureLoad(gbuf_depth, pixel, 0).r;
    let ray_dir = get_ray_dir(vec2<f32>(pixel), resolution);
    return camera.camera_pos + ray_dir * depth;
}

fn hash_noise(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
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
    let center_pos = camera.camera_pos + get_ray_dir(vec2<f32>(pixel), resolution) * center_depth;

    let num_slices = i32(gtao_params.ao_samples);
    let num_steps = i32(gtao_params.ao_steps);
    let f_num_slices = f32(num_slices);
    let f_num_steps = f32(num_steps);
    let radius = gtao_params.ao_radius;

    // Scale radius to screen space based on distance
    let screen_radius = radius / (center_depth * camera.pixel_size);
    let clamped_screen_radius = clamp(screen_radius, 2.0, 128.0);

    // Noise for per-pixel rotation
    let noise_angle = hash_noise(vec2<f32>(f32(x), f32(y))) * PI;

    var total_ao = 0.0;

    for (var slice = 0; slice < num_slices; slice++) {
        let angle = (f32(slice) + 0.5) / f_num_slices * PI + noise_angle;
        let dir_2d = vec2<f32>(cos(angle), sin(angle));

        // Track horizon angles for both sides of the slice
        var horizon_pos: f32 = -1.0;
        var horizon_neg: f32 = -1.0;

        for (var step = 1; step <= num_steps; step++) {
            let t = f32(step) / f_num_steps;
            let offset = dir_2d * clamped_screen_radius * t;

            // Positive direction
            let sample_pos = vec2<i32>(pixel) + vec2<i32>(i32(round(offset.x)), i32(round(offset.y)));
            if sample_pos.x >= 0 && sample_pos.x < i32(dims.x) && sample_pos.y >= 0 && sample_pos.y < i32(dims.y) {
                let sample_depth = textureLoad(gbuf_depth, sample_pos, 0).r;
                if sample_depth > 0.0 {
                    let sample_world = camera.camera_pos + get_ray_dir(vec2<f32>(sample_pos), resolution) * sample_depth;
                    let horizon_vec = sample_world - center_pos;
                    let dist = length(horizon_vec);
                    if dist > 0.001 && dist < radius * 2.0 {
                        let h = dot(horizon_vec / dist, normal);
                        horizon_pos = max(horizon_pos, h);
                    }
                }
            }

            // Negative direction
            let sample_neg = vec2<i32>(pixel) - vec2<i32>(i32(round(offset.x)), i32(round(offset.y)));
            if sample_neg.x >= 0 && sample_neg.x < i32(dims.x) && sample_neg.y >= 0 && sample_neg.y < i32(dims.y) {
                let sample_depth = textureLoad(gbuf_depth, sample_neg, 0).r;
                if sample_depth > 0.0 {
                    let sample_world = camera.camera_pos + get_ray_dir(vec2<f32>(sample_neg), resolution) * sample_depth;
                    let horizon_vec = sample_world - center_pos;
                    let dist = length(horizon_vec);
                    if dist > 0.001 && dist < radius * 2.0 {
                        let h = dot(horizon_vec / dist, normal);
                        horizon_neg = max(horizon_neg, h);
                    }
                }
            }
        }

        // Integrate visibility: unoccluded hemisphere contributes 1.0
        // Horizon at angle h reduces visibility by integrating cos over the blocked region
        let vis_pos = 1.0 - max(horizon_pos, 0.0);
        let vis_neg = 1.0 - max(horizon_neg, 0.0);
        total_ao += (vis_pos + vis_neg) * 0.5;
    }

    total_ao /= f_num_slices;

    let ao = clamp(pow(total_ao, gtao_params.ao_intensity), 0.0, 1.0);

    textureStore(ao_output, pixel, vec4<f32>(ao, 0.0, 0.0, 0.0));
}
