@group(0) @binding(0) var gbuf_color: texture_2d<f32>;
@group(0) @binding(1) var gbuf_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuf_depth: texture_2d<f32>;
@group(0) @binding(3) var output_color: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<uniform> render_settings: RenderSettingsUniform;
@group(0) @binding(5) var ao_texture: texture_2d<f32>;
@group(0) @binding(6) var<uniform> camera: CameraUniform;

const UNDERWATER_SURFACE_DEPTH_MIN: f32 = 2.5;
const UNDERWATER_SURFACE_DEPTH_MAX: f32 = 18.0;
const UNDERWATER_NEAR_DEPTH_DOWN: f32 = 1.5;
const UNDERWATER_NEAR_DEPTH_UP: f32 = 3.5;

// Screen-space distortion strength in pixels when camera is underwater
const UNDERWATER_DISTORTION_PX: f32 = 30.0;

// Screen-space distortion for water surface viewed from above
const WATER_SURFACE_DISTORTION_PX: f32 = 24.0;

fn underwater_surface_depth(ray_dir: vec3<f32>) -> f32 {
    let up = clamp(ray_dir.y, 0.0, 1.0);
    return mix(UNDERWATER_SURFACE_DEPTH_MAX, UNDERWATER_SURFACE_DEPTH_MIN, sqrt(up));
}

fn underwater_distort_pixel(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<i32> {
    // Reconstruct ray direction for this pixel
    let uv_x = (f32(pixel.x) + 0.5) / camera.resolution.x;
    let uv_y = 1.0 - (f32(pixel.y) + 0.5) / camera.resolution.y;
    let ray_dir = normalize(camera.ray_corner + camera.ray_right * (uv_x * 2.0) + camera.ray_up * (uv_y * 2.0));

    // Estimate where this ray exits the water surface
    let surface_t = underwater_surface_depth(ray_dir);
    let surface_pos = camera.camera_pos + ray_dir * surface_t;

    // Sample noise in screen UV space so distortion doesn't speed up with camera movement
    let screen_uv = vec2<f32>(uv_x, uv_y);
    let scale1 = 3.0;
    let speed1 = 0.3;
    let scale2 = 7.0;
    let speed2 = 0.7;
    let p1 = screen_uv * scale1 + vec2<f32>(camera.time * speed1, camera.time * speed1 * 0.7);
    let p2 = screen_uv * scale2 + vec2<f32>(-camera.time * speed2 * 0.6, camera.time * speed2);
    let noise_x = (fbm2(p1) + fbm2(p2) * 0.5) - 0.5;
    let noise_y = (fbm2(p1 + vec2<f32>(7.3, 13.7)) + fbm2(p2 + vec2<f32>(7.3, 13.7)) * 0.5) - 0.5;

    // Fade distortion to zero near screen edges to avoid stretched sampling
    let edge = vec2<f32>(f32(pixel.x), f32(pixel.y));
    let margin = UNDERWATER_DISTORTION_PX;
    let fade_x = min(edge.x, f32(dims.x) - 1.0 - edge.x) / margin;
    let fade_y = min(edge.y, f32(dims.y) - 1.0 - edge.y) / margin;
    let fade = clamp(min(fade_x, fade_y), 0.0, 1.0);

    let offset = vec2<f32>(noise_x, noise_y) * UNDERWATER_DISTORTION_PX * fade;
    let distorted = clamp(
        edge + offset,
        vec2<f32>(0.0),
        vec2<f32>(f32(dims.x) - 1.0, f32(dims.y) - 1.0),
    );
    return vec2<i32>(i32(distorted.x), i32(distorted.y));
}

fn water_surface_distort_pixel(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<i32> {
    let uv_x = (f32(pixel.x) + 0.5) / camera.resolution.x;
    let uv_y = 1.0 - (f32(pixel.y) + 0.5) / camera.resolution.y;
    let ray_dir = normalize(camera.ray_corner + camera.ray_right * (uv_x * 2.0) + camera.ray_up * (uv_y * 2.0));

    // Use the water surface hit depth from the g-buffer to get the surface world position
    let scene_t = textureLoad(gbuf_depth, pixel, 0).r;
    let surface_pos = camera.camera_pos + ray_dir * scene_t;

    // Use world-space XZ of the water surface for noise so the wobble follows the waves
    let snapped_xz = floor(surface_pos.xz / 2.0 + 0.5) * 2.0;
    let scale1 = 0.02;
    let speed1 = 0.3;
    let scale2 = 0.08;
    let speed2 = 0.7;
    let p1 = snapped_xz * scale1 + vec2<f32>(camera.time * speed1, camera.time * speed1 * 0.7);
    let p2 = snapped_xz * scale2 + vec2<f32>(-camera.time * speed2 * 0.6, camera.time * speed2);
    let noise_x = (fbm2(p1) + fbm2(p2) * 0.5) - 0.5;
    let noise_y = (fbm2(p1 + vec2<f32>(7.3, 13.7)) + fbm2(p2 + vec2<f32>(7.3, 13.7)) * 0.5) - 0.5;

    // Fade near screen edges
    let edge = vec2<f32>(f32(pixel.x), f32(pixel.y));
    let margin = WATER_SURFACE_DISTORTION_PX;
    let fade_x = min(edge.x, f32(dims.x) - 1.0 - edge.x) / margin;
    let fade_y = min(edge.y, f32(dims.y) - 1.0 - edge.y) / margin;
    let fade = clamp(min(fade_x, fade_y), 0.0, 1.0);

    let offset = vec2<f32>(noise_x, noise_y) * WATER_SURFACE_DISTORTION_PX * fade;
    let distorted = clamp(
        edge + offset,
        vec2<f32>(0.0),
        vec2<f32>(f32(dims.x) - 1.0, f32(dims.y) - 1.0),
    );
    return vec2<i32>(i32(distorted.x), i32(distorted.y));
}

fn underwater_view_depth(ray_dir: vec3<f32>, scene_t: f32) -> f32 {
    if camera.camera_underwater <= 0.5 {
        return scene_t;
    }
    return min(scene_t, underwater_surface_depth(ray_dir));
}

fn apply_underwater_lighting(color: vec3<f32>, ray_dir: vec3<f32>, scene_t: f32) -> vec3<f32> {
    let up = clamp(ray_dir.y, 0.0, 1.0);
    let min_depth = mix(UNDERWATER_NEAR_DEPTH_DOWN, UNDERWATER_NEAR_DEPTH_UP, sqrt(up));
    return water_absorb(color, max(underwater_view_depth(ray_dir, scene_t), min_depth));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(gbuf_color);

    let x = gid.x;
    let y = gid.y;
    if x >= dims.x || y >= dims.y { return; }

    let pixel = vec2<i32>(i32(x), i32(y));

    // When underwater, distort all g-buffer reads for a consistent wobble effect
    var sample_pixel = pixel;
    if camera.camera_underwater > 0.5 {
        sample_pixel = underwater_distort_pixel(pixel, dims);
    }

    let normal_sample = textureLoad(gbuf_normal, sample_pixel, 0);
    let hit_flag = normal_sample.w;

    if hit_flag <= 0.0 {
        // Reconstruct ray direction (must match trace.wgsl)
        let uv_x = (f32(x) + 0.5) / camera.resolution.x;
        let uv_y = 1.0 - (f32(y) + 0.5) / camera.resolution.y;
        let ray_dir = normalize(camera.ray_corner + camera.ray_right * (uv_x * 2.0) + camera.ray_up * (uv_y * 2.0));
        let sun_dir = normalize(render_settings.sun_direction.xyz);
        let sky = sky_color(ray_dir, sun_dir);
        textureStore(output_color, pixel, vec4<f32>(sky, 1.0));
        return;
    }

    let color_sample = textureLoad(gbuf_color, sample_pixel, 0);
    let base_color = color_sample.rgb;
    let shadow = color_sample.a;
    let normal = normal_sample.xyz;

    let ao = textureLoad(ao_texture, sample_pixel, 0).r;

    // Water pixels (normal.w ~= 0.5) are pre-lit in trace pass —
    // only apply shadow darkening, skip AO and diffuse lighting.
    if hit_flag > 0.3 && hit_flag < 0.7 {
        // When viewing water from above, distort the sample to create a refractive wobble
        var water_sample_pixel = sample_pixel;
        if camera.camera_underwater <= 0.5 {
            water_sample_pixel = water_surface_distort_pixel(pixel, dims);
            // Only use distorted pixel if it's also water, to avoid bleeding land into water
            let distorted_flag = textureLoad(gbuf_normal, water_sample_pixel, 0).w;
            if distorted_flag <= 0.3 || distorted_flag >= 0.7 {
                water_sample_pixel = sample_pixel;
            }
        }
        let water_color_sample = textureLoad(gbuf_color, water_sample_pixel, 0);
        // Water: color is already Fresnel-blended reflection+refraction+specular.
        // Modulate by shadow only (shadow is in color_sample.a).
        let water_light = mix(0.5, 1.0, water_color_sample.a);
        var water_color = water_color_sample.rgb * water_light;
        if camera.camera_underwater > 0.5 {
            let uv_x = (f32(x) + 0.5) / camera.resolution.x;
            let uv_y = 1.0 - (f32(y) + 0.5) / camera.resolution.y;
            let ray_dir = normalize(camera.ray_corner + camera.ray_right * (uv_x * 2.0) + camera.ray_up * (uv_y * 2.0));
            let scene_t = textureLoad(gbuf_depth, sample_pixel, 0).r;
            water_color = apply_underwater_lighting(water_color, ray_dir, scene_t);
        }
        textureStore(output_color, pixel, vec4<f32>(water_color, 1.0));
        return;
    }

    let sun_dir = normalize(render_settings.sun_direction.xyz);
    let n_dot_l = max(dot(normal, sun_dir), 0.0);
    let light = render_settings.ambient_light * ao + render_settings.sun_contribution * n_dot_l * shadow;
    var lit_color = base_color * light;
    if camera.camera_underwater > 0.5 {
        let uv_x = (f32(x) + 0.5) / camera.resolution.x;
        let uv_y = 1.0 - (f32(y) + 0.5) / camera.resolution.y;
        let ray_dir = normalize(camera.ray_corner + camera.ray_right * (uv_x * 2.0) + camera.ray_up * (uv_y * 2.0));
        let scene_t = textureLoad(gbuf_depth, sample_pixel, 0).r;
        lit_color = apply_underwater_lighting(lit_color, ray_dir, scene_t);
    }

    textureStore(output_color, pixel, vec4<f32>(lit_color, 1.0));
}
