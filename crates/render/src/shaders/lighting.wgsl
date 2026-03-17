@group(0) @binding(0) var gbuf_color: texture_2d<f32>;
@group(0) @binding(1) var gbuf_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuf_depth: texture_2d<f32>;
@group(0) @binding(3) var output_color: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<uniform> render_settings: RenderSettingsUniform;
@group(0) @binding(5) var ao_texture: texture_2d<f32>;

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
        textureStore(output_color, pixel, vec4<f32>(render_settings.sky_color.rgb, 1.0));
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
