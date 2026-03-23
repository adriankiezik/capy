@group(0) @binding(0) var gbuf_color: texture_2d<f32>;
@group(0) @binding(1) var gbuf_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuf_depth: texture_2d<f32>;
@group(0) @binding(3) var output_color: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<uniform> render_settings: RenderSettingsUniform;
@group(0) @binding(5) var ao_texture: texture_2d<f32>;
@group(0) @binding(6) var<uniform> camera: CameraUniform;

// Analytical atmospheric scattering sky model (based on shff/opengl_sky)
const SKY_PI: f32 = 3.14159265;
const SKY_BR: f32 = 0.0025;
const SKY_BM: f32 = 0.0003;
const SKY_G: f32 = 0.98;
const NITROGEN: vec3<f32> = vec3<f32>(0.650, 0.570, 0.475);

// Pixelation grid size for sun glow (higher = bigger pixels)
const SUN_PIXEL_SCALE: f32 = 128.0;

fn pixelate_dir(dir: vec3<f32>) -> vec3<f32> {
    // Snap ray direction to a coarse angular grid
    return normalize(floor(dir * SUN_PIXEL_SCALE + 0.5) / SUN_PIXEL_SCALE);
}

fn sky_color(ray_dir: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let pos = normalize(ray_dir);
    let fsun = normalize(sun_dir);

    let Kr = vec3<f32>(SKY_BR) / pow(NITROGEN, vec3<f32>(4.0));
    let Km = vec3<f32>(SKY_BM) / pow(NITROGEN, vec3<f32>(0.84));

    // Smooth ray for gradient
    let mu = dot(pos, fsun);
    let rayleigh = 3.0 / (8.0 * SKY_PI) * (1.0 + mu * mu);

    // Pixelated ray for sun glow only
    let pos_px = pixelate_dir(ray_dir);
    let mu_px = dot(pos_px, fsun);

    let mie = (Kr + Km * (1.0 - SKY_G * SKY_G) / (2.0 + SKY_G * SKY_G)
              / pow(1.0 + SKY_G * SKY_G - 2.0 * SKY_G * mu_px, 1.5))
              / (SKY_BR + SKY_BM);

    let day_extinction = exp(
        -exp(-((pos.y + fsun.y * 4.0) * (exp(-pos.y * 16.0) + 0.1) / 80.0) / SKY_BR)
        * (exp(-pos.y * 16.0) + 0.1) * Kr / SKY_BR
    ) * exp(-pos.y * exp(-pos.y * 8.0) * 4.0)
      * exp(-pos.y * 1.3) * 1.7;

    let night_extinction = vec3<f32>(1.0 - exp(fsun.y)) * 0.2;
    let extinction = mix(day_extinction, night_extinction, -fsun.y * 0.2 + 0.5);

    return rayleigh * mie * extinction;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(gbuf_color);

    let x = gid.x;
    let y = gid.y;
    if x >= dims.x || y >= dims.y { return; }

    let pixel = vec2<i32>(i32(x), i32(y));

    let normal_sample = textureLoad(gbuf_normal, pixel, 0);
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

    let color_sample = textureLoad(gbuf_color, pixel, 0);
    let base_color = color_sample.rgb;
    let shadow = color_sample.a;
    let normal = normal_sample.xyz;

    let ao = textureLoad(ao_texture, pixel, 0).r;

    let sun_dir = normalize(render_settings.sun_direction.xyz);
    let n_dot_l = max(dot(normal, sun_dir), 0.0);
    let light = render_settings.ambient_light * ao + render_settings.sun_contribution * n_dot_l * shadow;

    textureStore(output_color, pixel, vec4<f32>(base_color * light, 1.0));
}
