@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> render_settings: RenderSettingsUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) material: u32,
    @location(3) instance_offset: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) material: u32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world_position = input.position + input.instance_offset.xyz;
    output.clip_position = camera.clip_from_world * vec4<f32>(world_position, 1.0);
    output.world_position = world_position;
    output.normal = input.normal;
    output.material = input.material;
    return output;
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) ray_depth: f32,
};

@fragment
fn fs_opaque(input: VertexOutput) -> FragmentOutput {
    if (input.material & 0x4000u) != 0u {
        discard;
    }
    var output: FragmentOutput;
    output.color = vec4<f32>(
        render_settings.material_colors[min(input.material & 0x3FFFu, 1023u)].rgb,
        1.0,
    );
    output.normal = vec4<f32>(input.normal, 1.0);
    output.ray_depth = length(input.world_position - camera.camera_pos);
    return output;
}

struct WaterFragmentOutput {
    @location(0) normal: vec4<f32>,
    @location(1) ray_depth: f32,
};

@fragment
fn fs_water(input: VertexOutput) -> WaterFragmentOutput {
    if (input.material & 0x4000u) == 0u {
        discard;
    }
    var output: WaterFragmentOutput;
    output.normal = vec4<f32>(input.normal, 1.0);
    output.ray_depth = length(input.world_position - camera.camera_pos);
    return output;
}
