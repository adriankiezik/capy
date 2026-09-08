@group(0) @binding(0) var<uniform> screen: vec4<f32>;

struct Output {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex fn vs_main(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> Output {
    var out: Output;

    out.clip = vec4(position.x / screen.x * 2.0 - 1.0, 1.0 - position.y / screen.y * 2.0, 0.0, 1.0);
    out.color = color;

    return out;
}

@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> {
    return input.color;
}
