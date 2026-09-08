struct Light {
    matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> light: Light;

@vertex fn vs_main(@location(0) position: vec3<f32>, @location(3) translation: vec4<f32>) -> @builtin(position) vec4<f32> {
    return light.matrix * vec4(position + translation.xyz, 1.0);
}
