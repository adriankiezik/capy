@group(0) @binding(0) var gbuf_color_out: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(7) var gbuf_depth_out: texture_storage_2d<r32float, write>;
@group(0) @binding(9) var gbuf_normal_out: texture_storage_2d<rgba8snorm, write>;
@group(0) @binding(10) var dlss_depth_out: texture_storage_2d<r32float, write>;
@group(0) @binding(11) var motion_vectors_out: texture_storage_2d<rg32float, write>;
struct TraceStats {
    primary_chunk_steps: atomic<u32>,
    primary_node_steps: atomic<u32>,
    primary_descents: atomic<u32>,
    shadow_chunk_steps: atomic<u32>,
    shadow_node_steps: atomic<u32>,
    shadow_descents: atomic<u32>,
    hit_pixels: atomic<u32>,
    miss_pixels: atomic<u32>,
    shadow_rays: atomic<u32>,
    shadow_blocked: atomic<u32>,
    lod_hits: atomic<u32>,
    material_hits: atomic<u32>,
};
@group(0) @binding(12) var<storage, read_write> trace_stats: TraceStats;

@group(0) @binding(1) var<uniform> camera: CameraUniform;
@group(0) @binding(2) var<uniform> streaming: StreamingInfo;
@group(0) @binding(3) var<storage, read> chunk_pool: array<u32>;
@group(0) @binding(4) var<storage, read> chunk_avg_pool: array<u32>;
@group(0) @binding(5) var<storage, read> indirection: array<u32>;
@group(0) @binding(6) var<storage, read_write> lod_debug_buf: array<u32>;
@group(0) @binding(8) var<uniform> render_settings: RenderSettingsUniform;

fn commit_trace_stats(
    hit: HitResult,
    shadow_rays: u32,
    shadow_blocked: u32,
) {
    atomicAdd(&trace_stats.primary_chunk_steps, trace_stats_primary_chunk_steps);
    atomicAdd(&trace_stats.primary_node_steps, trace_stats_primary_node_steps);
    atomicAdd(&trace_stats.primary_descents, trace_stats_primary_descents);
    atomicAdd(&trace_stats.shadow_chunk_steps, trace_stats_shadow_chunk_steps);
    atomicAdd(&trace_stats.shadow_node_steps, trace_stats_shadow_node_steps);
    atomicAdd(&trace_stats.shadow_descents, trace_stats_shadow_descents);
    if hit.hit {
        atomicAdd(&trace_stats.hit_pixels, 1u);
        if hit.is_lod_hit {
            atomicAdd(&trace_stats.lod_hits, 1u);
        } else {
            atomicAdd(&trace_stats.material_hits, 1u);
        }
    } else {
        atomicAdd(&trace_stats.miss_pixels, 1u);
    }
    if shadow_rays > 0u {
        atomicAdd(&trace_stats.shadow_rays, shadow_rays);
    }
    if shadow_blocked > 0u {
        atomicAdd(&trace_stats.shadow_blocked, shadow_blocked);
    }
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(gbuf_color_out);

    let actual_x = gid.x;
    let actual_y = gid.y;
    if actual_x >= dims.x || actual_y >= dims.y { return; }

    reset_trace_private_stats();

    let uv_x = (f32(actual_x) + 0.5 + camera.jitter.x) / camera.resolution.x;
    let uv_y = 1.0 - (f32(actual_y) + 0.5 + camera.jitter.y) / camera.resolution.y;

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
    var shadow_ray_count = 0u;
    var shadow_blocked_count = 0u;
    if hit.hit {
        var base: vec3<f32>;
        if hit.is_lod_hit {
            base = hit.color_override;
        } else {
            base = render_settings.material_colors[min(hit.material, 1023u)].rgb;
        }

        var shadow = 1.0;
        if render_settings.sun_contribution > 0.0 {
            let sun_dir = normalize(render_settings.sun_direction.xyz);
            let shadow_origin = hit.hit_pos_local + hit.normal * render_settings.ray_epsilon;
            let in_shadow = trace_shadow_ray(shadow_origin, sun_dir);
            shadow_ray_count = 1u;
            shadow_blocked_count = select(0u, 1u, in_shadow);
            shadow = select(1.0, 0.0, in_shadow);
        }
        let clip_pos = camera.clip_from_world * vec4<f32>(hit.hit_pos_local, 1.0);
        let prev_clip_pos = camera.prev_clip_from_world * vec4<f32>(hit.hit_pos_local, 1.0);
        let curr_ndc = clip_pos.xy / clip_pos.w;
        let prev_ndc = prev_clip_pos.xy / prev_clip_pos.w;
        let motion = (curr_ndc - prev_ndc) * vec2<f32>(0.5, -0.5);
        let hardware_depth = clamp(clip_pos.z / clip_pos.w, 0.0, 1.0);

        textureStore(gbuf_color_out, pixel, vec4<f32>(base, shadow));
        textureStore(gbuf_normal_out, pixel, vec4<f32>(hit.normal, 1.0));
        let depth_val = length(hit.hit_pos_local - ray_origin);
        textureStore(gbuf_depth_out, pixel, vec4<f32>(depth_val, 0.0, 0.0, 0.0));
        textureStore(dlss_depth_out, pixel, vec4<f32>(hardware_depth, 0.0, 0.0, 0.0));
        textureStore(motion_vectors_out, pixel, vec4<f32>(motion, 0.0, 0.0));
    } else {
        textureStore(gbuf_color_out, pixel, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        textureStore(gbuf_normal_out, pixel, vec4<f32>(0.0, 0.0, 0.0, -1.0));
        textureStore(gbuf_depth_out, pixel, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        textureStore(dlss_depth_out, pixel, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        textureStore(motion_vectors_out, pixel, vec4<f32>(0.0, 0.0, 0.0, 0.0));
    }

    commit_trace_stats(hit, shadow_ray_count, shadow_blocked_count);
}
