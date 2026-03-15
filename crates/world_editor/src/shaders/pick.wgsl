@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> streaming: StreamingInfo;
@group(0) @binding(2) var<storage, read> chunk_pool: array<u32>;
@group(0) @binding(3) var<storage, read> chunk_avg_pool: array<u32>;
@group(0) @binding(4) var<storage, read> indirection: array<u32>;
@group(0) @binding(5) var<uniform> render_settings: RenderSettingsUniform;

struct PickInput {
    pixel_x: u32,
    pixel_y: u32,
};

struct PickOutput {
    hit: u32,
    pos_x: f32,
    pos_y: f32,
    pos_z: f32,
    normal_x: f32,
    normal_y: f32,
    normal_z: f32,
    material: u32,
};

@group(0) @binding(6) var<uniform> pick_input: PickInput;
@group(0) @binding(7) var<storage, read_write> pick_output: PickOutput;

@compute @workgroup_size(1)
fn main() {
    let uv_x = (f32(pick_input.pixel_x) + 0.5) / camera.resolution.x;
    let uv_y = 1.0 - (f32(pick_input.pixel_y) + 0.5) / camera.resolution.y;

    let ray_dir = normalize(
        camera.ray_corner
        + camera.ray_right * (uv_x * 2.0)
        + camera.ray_up * (uv_y * 2.0)
    );
    let ray_origin = camera.camera_pos;

    let hit = trace_ray(ray_origin, ray_dir);

    if hit.hit {
        pick_output.hit = 1u;
        pick_output.pos_x = hit.hit_pos_local.x;
        pick_output.pos_y = hit.hit_pos_local.y;
        pick_output.pos_z = hit.hit_pos_local.z;
        pick_output.normal_x = hit.normal.x;
        pick_output.normal_y = hit.normal.y;
        pick_output.normal_z = hit.normal.z;
        pick_output.material = select(hit.material, 0u, hit.is_lod_hit);
    } else {
        pick_output.hit = 0u;
        pick_output.pos_x = 0.0;
        pick_output.pos_y = 0.0;
        pick_output.pos_z = 0.0;
        pick_output.normal_x = 0.0;
        pick_output.normal_y = 0.0;
        pick_output.normal_z = 0.0;
        pick_output.material = 0u;
    }
}
