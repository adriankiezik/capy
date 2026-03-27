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
    grass_trace_calls: atomic<u32>,
    grass_run_visits: atomic<u32>,
    grass_steps: atomic<u32>,
    grass_candidates: atomic<u32>,
    grass_tile_rejects: atomic<u32>,
    grass_heightmap_reads: atomic<u32>,
    grass_column_misses: atomic<u32>,
    grass_y_checks: atomic<u32>,
    grass_y_rejects: atomic<u32>,
    grass_trace_hits: atomic<u32>,
    grass_visible_pixels: atomic<u32>,
    grass_shadow_rays: atomic<u32>,
    water_pixels: atomic<u32>,
    water_top_face_pixels: atomic<u32>,
    water_side_face_pixels: atomic<u32>,
    water_shadow_rays: atomic<u32>,
    water_absorb_evals: atomic<u32>,
    water_underwater_sky: atomic<u32>,
    water_dda_chunks_behind: atomic<u32>,
    water_deep_no_hit: atomic<u32>,
    water_normal_evals: atomic<u32>,
    water_sky_evals: atomic<u32>,
    water_normal_lod: atomic<u32>,
    water_shadow_skipped: atomic<u32>,
};
@group(0) @binding(12) var<storage, read_write> trace_stats: TraceStats;

@group(0) @binding(1) var<uniform> camera: CameraUniform;
@group(0) @binding(2) var<uniform> streaming: StreamingInfo;
@group(0) @binding(3) var<storage, read> chunk_pool: array<u32>;
@group(0) @binding(4) var<storage, read> chunk_avg_pool: array<u32>;
@group(0) @binding(5) var<storage, read> indirection: array<u32>;
@group(0) @binding(6) var<storage, read_write> lod_debug_buf: array<u32>;
@group(0) @binding(8) var<uniform> render_settings: RenderSettingsUniform;

struct PreviewParams {
    is_active: u32,
    pool_offset: u32,
    world_size: u32,
    root_offset: u32,
    depth: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    pos_x: f32,
    pos_y: f32,
    pos_z: f32,
    tint_strength: f32,
    tint_r: f32,
    tint_g: f32,
    tint_b: f32,
    _pad3: f32,
};
@group(0) @binding(13) var<uniform> preview: PreviewParams;

struct SelectionParams {
    aabb_min: vec3<f32>,
    is_active: u32,
    aabb_max: vec3<f32>,
    _pad0: u32,
};
@group(0) @binding(14) var<uniform> selection: SelectionParams;

fn apply_selection_tint(base: vec3<f32>, pos: vec3<f32>) -> vec3<f32> {
    if selection.is_active == 0u {
        return base;
    }
    let inside = all(pos >= selection.aabb_min) && all(pos <= selection.aabb_max);
    if inside {
        // Bright cyan tint on selected voxels
        return mix(base, vec3<f32>(0.3, 0.7, 1.0), 0.3);
    }
    // Darken and desaturate voxels outside the selection
    let grey = dot(base, vec3<f32>(0.299, 0.587, 0.114));
    let desaturated = mix(base, vec3<f32>(grey), 0.4);
    return desaturated * 0.7;
}
