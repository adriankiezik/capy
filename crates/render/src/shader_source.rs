pub const SHADER_BLIT: &str = include_str!("shaders/blit.wgsl");
const SHADER_STREAMING: &str = include_str!("shaders/streaming.wgsl");

const COMMON_CAMERA: &str = include_str!("shaders/common/camera.wgsl");
const COMMON_AABB: &str = include_str!("shaders/common/aabb.wgsl");
const COMMON_MASK64: &str = include_str!("shaders/common/mask64.wgsl");
const COMMON_RENDER_SETTINGS: &str = include_str!("shaders/common/render_settings.wgsl");
const COMMON_TRAVERSAL: &str = include_str!("shaders/common/traversal.wgsl");

pub fn create_compute_shader(
    device: &wgpu::Device,
    label: &str,
    custom_source: &str,
) -> wgpu::ShaderModule {
    let mut source = build_common_prefix();
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

pub(crate) fn build_streaming_shader_source() -> String {
    let mut out = build_common_prefix();
    out.push_str(SHADER_STREAMING.trim_start_matches('\u{feff}'));
    out
}
