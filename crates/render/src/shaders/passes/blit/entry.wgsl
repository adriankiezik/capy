struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct TonemappingParams {
    mode: u32,
    exposure: f32,
    _pad0: u32,
    _pad1: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@group(0) @binding(0) var blit_tex: texture_2d<f32>;
@group(0) @binding(1) var blit_sampler: sampler;
@group(0) @binding(2) var<uniform> tonemapping: TonemappingParams;

// ACES filmic tone mapping (Stephen Hill's fit)
fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return saturate((x * (a * x + b)) / (x * (c * x + d) + e));
}

// Reinhard tone mapping
fn reinhard(x: vec3<f32>) -> vec3<f32> {
    return x / (vec3<f32>(1.0) + x);
}

// Uncharted 2 tone mapping (John Hable)
fn uncharted2_partial(x: vec3<f32>) -> vec3<f32> {
    let A = 0.15;
    let B = 0.50;
    let C = 0.10;
    let D = 0.20;
    let E = 0.02;
    let F = 0.30;
    return ((x * (A * x + C * B) + D * E) / (x * (A * x + B) + D * F)) - E / F;
}

fn uncharted2(x: vec3<f32>) -> vec3<f32> {
    let W = 11.2;
    let curr = uncharted2_partial(x * 2.0);
    let white_scale = vec3<f32>(1.0) / uncharted2_partial(vec3<f32>(W));
    return curr * white_scale;
}

fn apply_tonemapping(color: vec3<f32>, mode: u32, exposure: f32) -> vec3<f32> {
    let exposed = color * exposure;
    switch mode {
        case 1u: { return aces(exposed); }
        case 2u: { return reinhard(exposed); }
        case 3u: { return uncharted2(exposed); }
        default: { return color; }
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(blit_tex, blit_sampler, in.uv);
    let mapped = apply_tonemapping(color.rgb, tonemapping.mode, tonemapping.exposure);
    return vec4<f32>(mapped, color.a);
}
