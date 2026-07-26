// ── lib/ — pure shared functions (math, noise, sky, camera) ──
const LIB_CAMERA: &str = include_str!("shaders/lib/camera.wgsl");
const LIB_AABB: &str = include_str!("shaders/lib/aabb.wgsl");
const LIB_MASK64: &str = include_str!("shaders/lib/mask64.wgsl");
const LIB_GRID_MATH: &str = include_str!("shaders/lib/grid_math.wgsl");
const LIB_NOISE: &str = include_str!("shaders/lib/noise.wgsl");
const LIB_SKY: &str = include_str!("shaders/lib/sky.wgsl");

// ── scene/ — data types, bindings, scene access ──
const SCENE_TRACE_STATS_CONFIG: &str = include_str!("shaders/scene/trace_stats_config.wgsl");
const SCENE_RENDER_SETTINGS: &str = include_str!("shaders/scene/render_settings.wgsl");
const SCENE_TYPES: &str = include_str!("shaders/scene/types.wgsl");
const SCENE_POOL_ACCESS: &str = include_str!("shaders/scene/pool_access.wgsl");
const SCENE_WATER: &str = include_str!("shaders/scene/water.wgsl");

// ── traversal/ — ray traversal ──
const TRAV_STATE: &str = include_str!("shaders/traversal/state.wgsl");
const TRAV_GRASS: &str = include_str!("shaders/traversal/grass.wgsl");
const TRAV_CHUNK_DDA: &str = include_str!("shaders/traversal/chunk_dda.wgsl");
const TRAV_CHUNK_PRIMARY: &str = include_str!("shaders/traversal/chunk_primary.wgsl");
const TRAV_CHUNK_SHADOW: &str = include_str!("shaders/traversal/chunk_shadow.wgsl");
const TRAV_RAY_PRIMARY: &str = include_str!("shaders/traversal/ray_primary.wgsl");
const TRAV_RAY_SHADOW: &str = include_str!("shaders/traversal/ray_shadow.wgsl");
const TRAV_RAY_REFLECTION: &str = include_str!("shaders/traversal/ray_reflection.wgsl");

// ── passes/trace/ — trace pass ──
const TRACE_BINDINGS: &str = include_str!("shaders/passes/trace/bindings.wgsl");
const TRACE_GBUFFER_WRITE: &str = include_str!("shaders/passes/trace/gbuffer_write.wgsl");
const TRACE_STATS_COMMIT: &str = include_str!("shaders/passes/trace/stats_commit.wgsl");
const TRACE_HIT_SELECTION: &str = include_str!("shaders/passes/trace/hit_selection.wgsl");
const TRACE_SHADE_WATER: &str = include_str!("shaders/passes/trace/shade_water.wgsl");
const TRACE_SHADE_SURFACE: &str = include_str!("shaders/passes/trace/shade_surface.wgsl");
const TRACE_ENTRY: &str = include_str!("shaders/passes/trace/entry.wgsl");

// ── passes/ — other passes ──
const PASS_LIGHTING: &str = include_str!("shaders/passes/lighting/entry.wgsl");
const PASS_GTAO: &str = include_str!("shaders/passes/gtao/entry.wgsl");
const PASS_NEAR_MESH: &str = include_str!("shaders/passes/near_mesh.wgsl");
pub const SHADER_BLIT: &str = include_str!("shaders/passes/blit/entry.wgsl");

const VOXEL_SCENE_BINDINGS: &str = r"
@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> streaming: StreamingInfo;
@group(0) @binding(2) var<storage, read> chunk_pool: array<u32>;
@group(0) @binding(3) var<storage, read> chunk_avg_pool: array<u32>;
@group(0) @binding(4) var<storage, read> indirection: array<u32>;
@group(0) @binding(5) var<uniform> render_settings: RenderSettingsUniform;
";

pub fn create_compute_shader(
    device: &wgpu::Device,
    label: &str,
    custom_source: &str,
) -> wgpu::ShaderModule {
    let mut source = build_common_prefix();
    source.push_str(VOXEL_SCENE_BINDINGS);
    source.push('\n');
    source.push_str(custom_source);
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

fn build_common_prefix() -> String {
    let mut out = String::new();
    // lib (pure functions)
    out.push_str(SCENE_TRACE_STATS_CONFIG);
    out.push('\n');
    out.push_str(LIB_CAMERA);
    out.push('\n');
    out.push_str(LIB_AABB);
    out.push('\n');
    out.push_str(LIB_MASK64);
    out.push('\n');
    out.push_str(LIB_NOISE);
    out.push('\n');
    out.push_str(LIB_SKY);
    out.push('\n');
    // scene (data types + access)
    out.push_str(SCENE_RENDER_SETTINGS);
    out.push('\n');
    out.push_str(TRAV_GRASS);
    out.push('\n');
    out.push_str(SCENE_WATER);
    out.push('\n');
    // traversal (types → math → access → state → chunks → rays)
    out.push_str(SCENE_TYPES);
    out.push('\n');
    out.push_str(LIB_GRID_MATH);
    out.push('\n');
    out.push_str(SCENE_POOL_ACCESS);
    out.push('\n');
    out.push_str(TRAV_STATE);
    out.push('\n');
    out.push_str(TRAV_CHUNK_DDA);
    out.push('\n');
    out.push_str(TRAV_CHUNK_PRIMARY);
    out.push('\n');
    out.push_str(TRAV_CHUNK_SHADOW);
    out.push('\n');
    out.push_str(TRAV_RAY_PRIMARY);
    out.push('\n');
    out.push_str(TRAV_RAY_SHADOW);
    out.push('\n');
    out.push_str(TRAV_RAY_REFLECTION);
    out.push('\n');
    out
}

pub(crate) fn build_trace_shader_source() -> String {
    let mut out = build_common_prefix();
    out.push_str(TRACE_BINDINGS);
    out.push('\n');
    out.push_str(TRACE_GBUFFER_WRITE);
    out.push('\n');
    out.push_str(TRACE_STATS_COMMIT);
    out.push('\n');
    out.push_str(TRACE_SHADE_WATER);
    out.push('\n');
    out.push_str(TRACE_SHADE_SURFACE);
    out.push('\n');
    out.push_str(TRACE_HIT_SELECTION);
    out.push('\n');
    out.push_str(TRACE_ENTRY);
    out
}

pub(crate) fn build_lighting_shader_source() -> String {
    let mut out = String::new();
    out.push_str(LIB_CAMERA);
    out.push('\n');
    out.push_str(LIB_NOISE);
    out.push('\n');
    out.push_str(LIB_SKY);
    out.push('\n');
    out.push_str(SCENE_RENDER_SETTINGS);
    out.push('\n');
    out.push_str(SCENE_WATER);
    out.push('\n');
    out.push_str(PASS_LIGHTING.trim_start_matches('\u{feff}'));
    out
}

pub(crate) fn build_gtao_shader_source() -> String {
    let mut out = String::new();
    out.push_str(LIB_CAMERA);
    out.push('\n');
    out.push_str(PASS_GTAO.trim_start_matches('\u{feff}'));
    out
}

pub(crate) fn build_near_mesh_shader_source() -> String {
    let mut out = String::new();
    out.push_str(LIB_CAMERA);
    out.push('\n');
    out.push_str(SCENE_RENDER_SETTINGS);
    out.push('\n');
    out.push_str(PASS_NEAR_MESH);
    out
}
