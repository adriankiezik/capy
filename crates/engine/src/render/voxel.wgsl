struct Camera {
    matrix: mat4x4<f32>,
    eye_fog: vec4<f32>,
    sky_ambient: vec4<f32>,
    sun: vec4<f32>,
    screen: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct Output {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
}

@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) translation: vec4<f32>) -> Output {
    var out: Output;

    out.position = position + translation.xyz;
    out.clip = camera.matrix * vec4(out.position, 1.0);
    out.normal = normal;
    out.color = color;

    return out;
}

@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> {
    if dot(input.normal, input.normal) < 0.5 {
        return vec4(input.color, 1.0);
    }

    let sun = max(dot(input.normal, camera.sun.xyz), 0.0);

    let light = camera.sky_ambient.w + (1.0 - camera.sky_ambient.w) * sun;

    let grain = fract(sin(dot(floor(input.position * 10.0 + input.normal * 0.01), vec3(12.9898, 78.233, 45.164))) * 43758.5453);

    let color = input.color * light * (0.95 + grain * 0.1);

    let distance = length(input.position - camera.eye_fog.xyz);

    let fog = 1.0 - exp(-pow(distance / camera.eye_fog.w, 2.0));

    return vec4(mix(color, camera.sky_ambient.xyz, fog), 1.0);
}

@vertex fn vs_hud(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>) -> Output {
    var out: Output;

    out.clip = vec4(position.x / camera.screen.x * 2.0 - 1.0, 1.0 - position.y / camera.screen.y * 2.0, 0.0, 1.0);
    out.position = position;
    out.normal = normal;
    out.color = color;

    return out;
}

@fragment fn fs_hud(input: Output) -> @location(0) vec4<f32> {
    return vec4(input.color, input.normal.x);
}
