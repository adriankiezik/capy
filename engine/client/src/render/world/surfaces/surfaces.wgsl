struct Camera {
    matrix: mat4x4<f32>,
    eye_fog: vec4<f32>,
    sky_ambient: vec4<f32>,
    sun: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct Shadows {
    matrices: array<mat4x4<f32>, 3>,
    splits: vec4<f32>,
    direction: vec4<f32>,
    parameters: vec4<f32>,
}

@group(1) @binding(0) var shadow_depth: texture_depth_2d_array;

@group(1) @binding(1) var shadow_sampler: sampler_comparison;

@group(1) @binding(2) var<uniform> shadows: Shadows;

struct Output {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) occlusion: f32,
    @location(4) surface_position: vec3<f32>,
}

@vertex fn vs_main(@location(0) position: vec3<f32>, @location(2) color: vec3<f32>, @location(3) translation: vec4<f32>, @location(4) shading: u32, @location(5) origin: vec4<f32>, @location(6) basis_x: vec4<f32>, @location(7) basis_y: vec4<f32>, @location(8) basis_z: vec4<f32>) -> Output {
    let normals = array<vec3<f32>, 6>(vec3(-1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, -1.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, -1.0), vec3(0.0, 0.0, 1.0));

    let normal = normals[shading & 7u];

    let occlusion = f32(shading >> 3u) / 3.0;

    var out: Output;

    let basis = mat3x3(basis_x.xyz, basis_y.xyz, basis_z.xyz);

    out.position = basis * position + translation.xyz;
    out.clip = camera.matrix * vec4(out.position, 1.0);
    out.normal = normalize(basis * normal);
    out.color = color;
    out.occlusion = occlusion;
    out.surface_position = position + origin.xyz;

    return out;
}

fn sample_shadow(position: vec3<f32>, cascade: u32) -> f32 {
    let light = shadows.matrices[cascade] * vec4(position, 1.0);

    let uv = light.xy * vec2(0.5, -0.5) + vec2(0.5);

    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) || light.z < 0.0 || light.z > 1.0 {
        return 1.0;
    }

    let offset = shadows.parameters.y * 0.5;

    return 0.25 * (
        textureSampleCompareLevel(shadow_depth, shadow_sampler, uv + vec2(-offset, -offset), i32(cascade), light.z)
        + textureSampleCompareLevel(shadow_depth, shadow_sampler, uv + vec2(offset, -offset), i32(cascade), light.z)
        + textureSampleCompareLevel(shadow_depth, shadow_sampler, uv + vec2(-offset, offset), i32(cascade), light.z)
        + textureSampleCompareLevel(shadow_depth, shadow_sampler, uv + vec2(offset, offset), i32(cascade), light.z)
    );
}

fn shadow_visibility(position: vec3<f32>) -> f32 {
    let distance = dot(position - camera.eye_fog.xyz, shadows.direction.xyz);

    if shadows.parameters.x == 0.0 || distance >= shadows.splits.z {
        return 1.0;
    }

    var cascade = 0u;

    if distance > shadows.splits.x {
        cascade = 1u;
    }

    if distance > shadows.splits.y {
        cascade = 2u;
    }

    let visibility = sample_shadow(position, cascade);

    let end = shadows.splits[cascade];

    let blend = smoothstep(end * 0.9, end, distance);

    if blend == 0.0 {
        return visibility;
    }

    if cascade == 2u {
        return mix(visibility, 1.0, blend);
    }

    return mix(visibility, sample_shadow(position, cascade + 1u), blend);
}

@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> {
    let sun = max(dot(input.normal, camera.sun.xyz), 0.0);

    var direct = 0.0;

    if sun > 0.0 && camera.sky_ambient.w < 1.0 {
        direct = sun * shadow_visibility(input.position);
    }

    let occlusion = select(1.0, input.occlusion, (u32(camera.sun.w) & 1u) != 0u);

    let light = camera.sky_ambient.w * mix(0.35, 1.0, occlusion) + (1.0 - camera.sky_ambient.w) * direct;

    var variation = 1.0;

    if (u32(camera.sun.w) & 2u) != 0u {
        let grain = fract(sin(dot(floor(input.surface_position * 10.0 + input.normal * 0.01), vec3(12.9898, 78.233, 45.164))) * 43758.5453);

        variation = 0.95 + grain * 0.1;
    }

    let color = input.color * light * variation;

    let distance = length(input.position - camera.eye_fog.xyz);

    let fog = 1.0 - exp(-pow(distance / camera.eye_fog.w, 2.0));

    return vec4(mix(color, camera.sky_ambient.xyz, fog), 1.0);
}
