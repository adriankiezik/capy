// Analytical Rayleigh + Mie sky model.

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
