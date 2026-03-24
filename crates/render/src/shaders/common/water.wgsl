// Water rendering utilities: noise-based animated normals, Fresnel, absorption, refraction.

// ---- simple hash-based noise (no texture dependency) ----

fn hash2(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn noise2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let a = hash2(i + vec2<f32>(0.0, 0.0));
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm2(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    for (var i = 0; i < 3; i++) {
        val += amp * noise2d(pos);
        amp *= 0.5;
        pos *= 2.01;
    }
    return val;
}

// ---- pixelation: snap world XZ to a grid so water matches voxel aesthetics ----

const WATER_PIXEL_SIZE: f32 = 2.0;  // world units per water "pixel"

fn snap_water_xz(world_xz: vec2<f32>) -> vec2<f32> {
    return floor(world_xz / WATER_PIXEL_SIZE + 0.5) * WATER_PIXEL_SIZE;
}

// ---- water surface normal from animated noise ----

fn water_normal(world_xz: vec2<f32>, time: f32) -> vec3<f32> {
    let scale1 = 0.02;
    let speed1 = 0.3;
    let scale2 = 0.08;
    let speed2 = 0.7;

    let p1 = world_xz * scale1 + vec2<f32>(time * speed1, time * speed1 * 0.7);
    let p2 = world_xz * scale2 + vec2<f32>(-time * speed2 * 0.6, time * speed2);

    let eps = 0.5;
    let h = fbm2(p1) + fbm2(p2) * 0.5;
    let hx = fbm2(p1 + vec2<f32>(eps, 0.0)) + fbm2(p2 + vec2<f32>(eps, 0.0)) * 0.5;
    let hz = fbm2(p1 + vec2<f32>(0.0, eps)) + fbm2(p2 + vec2<f32>(0.0, eps)) * 0.5;

    let dhdx = (hx - h) / eps;
    let dhdz = (hz - h) / eps;
    let strength = 0.15;

    return normalize(vec3<f32>(-dhdx * strength, 1.0, -dhdz * strength));
}

// ---- Foam at water edges (shoreline / shallow areas) ----

const FOAM_DEPTH_MAX: f32 = 3.0;   // underwater depth below which foam appears
const FOAM_COLOR: vec3<f32> = vec3<f32>(0.85, 0.9, 0.95);

fn water_foam(world_xz: vec2<f32>, time: f32, underwater_depth: f32) -> f32 {
    // No foam in deep water
    if underwater_depth >= FOAM_DEPTH_MAX { return 0.0; }

    // Depth-based intensity: strongest at very shallow, fades toward FOAM_DEPTH_MAX
    let depth_factor = 1.0 - underwater_depth / FOAM_DEPTH_MAX;
    let depth_fade = depth_factor * depth_factor;

    // Two layers of scrolling noise at different scales for organic foam look
    let p1 = world_xz * 0.15 + vec2<f32>(time * 0.12, -time * 0.08);
    let p2 = world_xz * 0.4 + vec2<f32>(-time * 0.07, time * 0.14);
    let n1 = fbm2(p1);
    let n2 = fbm2(p2);

    // Combine noises: foam appears where combined noise exceeds a depth-dependent threshold.
    // Shallower water → lower threshold → more foam coverage.
    let combined = n1 * 0.6 + n2 * 0.4;
    let threshold = mix(0.25, 0.55, 1.0 - depth_fade);
    let foam = smoothstep(threshold, threshold + 0.15, combined) * depth_fade;

    return foam;
}

// ---- Schlick Fresnel ----

fn schlick_fresnel(cos_theta: f32, f0: f32) -> f32 {
    let x = clamp(1.0 - cos_theta, 0.0, 1.0);
    let x2 = x * x;
    return f0 + (1.0 - f0) * x2 * x2 * x;
}

// ---- Beer's law absorption ----

const WATER_ABSORPTION: vec3<f32> = vec3<f32>(0.10, 0.02, 0.01);
const WATER_DEEP_COLOR: vec3<f32> = vec3<f32>(0.08, 0.25, 0.38);

fn water_absorb(underwater_color: vec3<f32>, depth: f32) -> vec3<f32> {
    let absorption = exp(-depth * WATER_ABSORPTION);
    return underwater_color * absorption + WATER_DEEP_COLOR * (1.0 - absorption);
}

// ---- Analytical sky model (shared with lighting.wgsl) ----
// Duplicated here so the trace shader can sample sky for water reflections.

const SKY_PI: f32 = 3.14159265;
const SKY_BR: f32 = 0.0025;
const SKY_BM: f32 = 0.0003;
const SKY_G: f32 = 0.98;
const NITROGEN: vec3<f32> = vec3<f32>(0.650, 0.570, 0.475);

// Pixelation grid size for sun glow (must match lighting.wgsl)
const SUN_PIXEL_SCALE: f32 = 128.0;

fn pixelate_dir(dir: vec3<f32>) -> vec3<f32> {
    return normalize(floor(dir * SUN_PIXEL_SCALE + 0.5) / SUN_PIXEL_SCALE);
}

fn sky_color(ray_dir: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let pos = normalize(ray_dir);
    let fsun = normalize(sun_dir);

    let Kr = vec3<f32>(SKY_BR) / pow(NITROGEN, vec3<f32>(4.0));
    let Km = vec3<f32>(SKY_BM) / pow(NITROGEN, vec3<f32>(0.84));

    // Smooth ray for gradient
    let mu = dot(pos, fsun);
    let rayleigh = 3.0 / (8.0 * SKY_PI) * (1.0 + mu * mu);

    // Pixelated ray for sun glow only
    let pos_px = pixelate_dir(ray_dir);
    let mu_px = dot(pos_px, fsun);

    let mie = (Kr + Km * (1.0 - SKY_G * SKY_G) / (2.0 + SKY_G * SKY_G)
              / pow(1.0 + SKY_G * SKY_G - 2.0 * SKY_G * mu_px, 1.5))
              / (SKY_BR + SKY_BM);

    let day_extinction = exp(
        -exp(-((pos.y + fsun.y * 4.0) * (exp(-pos.y * 16.0) + 0.1) / 80.0) / SKY_BR)
        * (exp(-pos.y * 16.0) + 0.1) * Kr / SKY_BR
    ) * exp(-pos.y * exp(-pos.y * 8.0) * 4.0)
      * exp(-pos.y * 1.3) * 1.7;

    let night_extinction = vec3<f32>(1.0 - exp(fsun.y)) * 0.2;
    let extinction = mix(day_extinction, night_extinction, -fsun.y * 0.2 + 0.5);

    return rayleigh * mie * extinction;
}
