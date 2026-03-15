pub const SHADER_BLIT: &str = include_str!("shaders/blit.wgsl");
const SHADER_TRACE: &str = include_str!("shaders/trace.wgsl");
const SHADER_LIGHTING: &str = include_str!("shaders/lighting.wgsl");

const COMMON_CAMERA: &str = include_str!("shaders/common/camera.wgsl");
const COMMON_AABB: &str = include_str!("shaders/common/aabb.wgsl");
const COMMON_MASK64: &str = include_str!("shaders/common/mask64.wgsl");
const COMMON_RENDER_SETTINGS: &str = include_str!("shaders/common/render_settings.wgsl");
const COMMON_TRAVERSAL: &str = include_str!("shaders/common/traversal.wgsl");

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
    out.push_str(COMMON_CAMERA);
    out.push('\n');
    out.push_str(COMMON_AABB);
    out.push('\n');
    out.push_str(COMMON_MASK64);
    out.push('\n');
    out.push_str(COMMON_RENDER_SETTINGS);
    out.push('\n');
    out.push_str(COMMON_TRAVERSAL);
    out.push('\n');
    out
}

pub(crate) fn build_trace_shader_source() -> String {
    let mut out = build_common_prefix();
    out.push_str(SHADER_TRACE.trim_start_matches('\u{feff}'));
    out
}

pub(crate) fn build_lighting_shader_source() -> String {
    let mut out = String::new();
    out.push_str(COMMON_RENDER_SETTINGS);
    out.push('\n');
    out.push_str(SHADER_LIGHTING.trim_start_matches('\u{feff}'));
    out
}
