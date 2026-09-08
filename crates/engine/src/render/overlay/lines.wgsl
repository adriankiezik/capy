struct View {
    matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> view: View;

struct Output {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> Output {
    var out: Output;

    out.clip = view.matrix * vec4(position, 1.0);
    out.color = color;

    return out;
}

@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> {
    return vec4(input.color, 1.0);
}
