pub const SHADER_BLIT: &str = include_str!("shaders/blit.wgsl");
const SHADER_STREAMING: &str = include_str!("shaders/streaming.wgsl");

const COMMON_CAMERA: &str = include_str!("shaders/common/camera.wgsl");
const COMMON_AABB: &str = include_str!("shaders/common/aabb.wgsl");
const COMMON_MASK64: &str = include_str!("shaders/common/mask64.wgsl");
const COMMON_RENDER_SETTINGS: &str = include_str!("shaders/common/render_settings.wgsl");

pub fn build_streaming_shader_source() -> String {
    let mut out = String::new();
    out.push_str(COMMON_CAMERA);
    out.push('\n');
    out.push_str(COMMON_AABB);
    out.push('\n');
    out.push_str(COMMON_MASK64);
    out.push('\n');
    out.push_str(COMMON_RENDER_SETTINGS);
    out.push('\n');
    out.push_str(SHADER_STREAMING.trim_start_matches('\u{feff}'));
    out
}
