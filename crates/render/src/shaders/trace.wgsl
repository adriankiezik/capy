@group(0) @binding(0) var gbuf_color_out: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(7) var gbuf_depth_out: texture_storage_2d<r32float, write>;
@group(0) @binding(9) var gbuf_normal_out: texture_storage_2d<rgba8snorm, write>;

@group(0) @binding(1) var<uniform> camera: CameraUniform;
@group(0) @binding(2) var<uniform> streaming: StreamingInfo;
@group(0) @binding(3) var<storage, read> chunk_pool: array<u32>;
@group(0) @binding(4) var<storage, read> chunk_avg_pool: array<u32>;
@group(0) @binding(5) var<storage, read> indirection: array<u32>;
@group(0) @binding(6) var<storage, read_write> lod_debug_buf: array<u32>;
@group(0) @binding(8) var<uniform> render_settings: RenderSettingsUniform;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(gbuf_color_out);

    let actual_x = gid.x;
    let actual_y = gid.y;
    if actual_x >= dims.x || actual_y >= dims.y { return; }

    let uv_x = (f32(actual_x) + 0.5) / camera.resolution.x;
    let uv_y = 1.0 - (f32(actual_y) + 0.5) / camera.resolution.y;

    let ray_dir = normalize(
        camera.ray_corner
        + camera.ray_right * (uv_x * 2.0)
        + camera.ray_up * (uv_y * 2.0)
    );
    let ray_origin = camera.camera_pos;

    let hit = trace_ray(ray_origin, ray_dir);

    let pixel_idx = actual_y * u32(camera.resolution.x) + actual_x;
    if pixel_idx < arrayLength(&lod_debug_buf) {
        lod_debug_buf[pixel_idx] = hit.lod_scale_exp;
    }

    let pixel = vec2<i32>(i32(actual_x), i32(actual_y));
    if hit.hit {
        var base: vec3<f32>;
        if hit.is_lod_hit {
            base = hit.color_override;
        } else {
            base = render_settings.material_colors[min(hit.material, 7u)].rgb;
        }
        textureStore(gbuf_color_out, pixel, vec4<f32>(base, 1.0));
        textureStore(gbuf_normal_out, pixel, vec4<f32>(hit.normal, 1.0));
        let depth_val = length(hit.hit_pos_local - ray_origin);
        textureStore(gbuf_depth_out, pixel, vec4<f32>(depth_val, 0.0, 0.0, 0.0));
    } else {
        textureStore(gbuf_color_out, pixel, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        textureStore(gbuf_normal_out, pixel, vec4<f32>(0.0, 0.0, 0.0, -1.0));
        textureStore(gbuf_depth_out, pixel, vec4<f32>(0.0, 0.0, 0.0, 0.0));
    }
}
