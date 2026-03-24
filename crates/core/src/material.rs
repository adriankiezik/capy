use std::sync::LazyLock;

pub type MaterialId = u16;

/// Bit flag stored in the high bit of a `MaterialId` to indicate foliage (grass).
/// The visual material is stored in the lower 14 bits.
pub const FOLIAGE_BIT: MaterialId = 0x8000;

/// Bit flag indicating a water voxel. Can be combined with a visual material
/// in the lower 14 bits for tinted water.
pub const WATER_BIT: MaterialId = 0x4000;

/// Returns `true` if the given material ID has the foliage flag set.
pub const fn is_foliage_material(id: MaterialId) -> bool {
    (id & FOLIAGE_BIT) != 0
}

/// Returns `true` if the given material ID has the water flag set.
pub const fn is_water_material(id: MaterialId) -> bool {
    (id & WATER_BIT) != 0
}

/// Strip the flag bits and return the visual material index (0–1023).
pub const fn visual_material(id: MaterialId) -> MaterialId {
    id & !(FOLIAGE_BIT | WATER_BIT)
}

pub const MATERIAL_PALETTE_SIZE: usize = 1024;

const RESERVED_COLORS: [[f32; 3]; 24] = [
    [0.0, 0.0, 0.0],
    [0.22, 0.58, 0.18],
    [0.34, 0.64, 0.20],
    [0.49, 0.38, 0.23],
    [0.63, 0.48, 0.28],
    [0.74, 0.69, 0.55],
    [0.94, 0.92, 0.86],
    [0.18, 0.28, 0.62],
    [0.26, 0.42, 0.78],
    [0.67, 0.17, 0.15],
    [0.80, 0.31, 0.19],
    [0.84, 0.62, 0.24],
    [0.46, 0.24, 0.09],
    [0.61, 0.40, 0.16],
    [0.15, 0.46, 0.44],
    [0.28, 0.60, 0.58],
    [0.45, 0.12, 0.34],
    [0.62, 0.23, 0.48],
    [0.24, 0.24, 0.24],
    [0.40, 0.40, 0.40],
    [0.56, 0.56, 0.56],
    [0.72, 0.72, 0.72],
    [0.86, 0.86, 0.86],
    [1.0, 1.0, 1.0],
];

/// 1024-color palette with a curated prefix plus a dense 10x10x10 RGB cube.
/// Index 0 is air and index 1 is the default terrain green.
pub const MATERIAL_COLORS: [[f32; 3]; MATERIAL_PALETTE_SIZE] = generate_palette();

const fn generate_palette() -> [[f32; 3]; MATERIAL_PALETTE_SIZE] {
    let mut palette = [[0.0f32; 3]; MATERIAL_PALETTE_SIZE];
    let mut idx = 0usize;
    while idx < RESERVED_COLORS.len() {
        palette[idx] = RESERVED_COLORS[idx];
        idx += 1;
    }

    let mut ri = 0u32;
    while ri < 10 {
        let mut gi = 0u32;
        while gi < 10 {
            let mut bi = 0u32;
            while bi < 10 {
                palette[idx] = [ri as f32 / 9.0, gi as f32 / 9.0, bi as f32 / 9.0];
                idx += 1;
                bi += 1;
            }
            gi += 1;
        }
        ri += 1;
    }

    palette
}

/// 32x32x32 lookup table mapping quantized RGB → nearest `MaterialId`.
/// Built once on first access (~33ms), then every lookup is O(1).
const LUT_RES: usize = 64;
const LUT_SIZE: usize = LUT_RES * LUT_RES * LUT_RES;

static MATERIAL_LUT: LazyLock<Vec<MaterialId>> = LazyLock::new(|| {
    let mut lut = vec![0u16; LUT_SIZE];
    for ri in 0..LUT_RES {
        for gi in 0..LUT_RES {
            for bi in 0..LUT_RES {
                let color = [
                    ri as f32 / (LUT_RES - 1) as f32,
                    gi as f32 / (LUT_RES - 1) as f32,
                    bi as f32 / (LUT_RES - 1) as f32,
                ];
                let index = ri * LUT_RES * LUT_RES + gi * LUT_RES + bi;
                lut[index] = closest_material_bruteforce(color);
            }
        }
    }
    lut
});

/// Find the closest palette entry to the given RGB color, skipping index 0 (air).
/// Uses a precomputed 32^3 LUT for O(1) lookups.
pub fn closest_material(color: [f32; 3]) -> MaterialId {
    let scale = (LUT_RES - 1) as f32;
    let ri = (color[0].clamp(0.0, 1.0) * scale + 0.5) as usize;
    let gi = (color[1].clamp(0.0, 1.0) * scale + 0.5) as usize;
    let bi = (color[2].clamp(0.0, 1.0) * scale + 0.5) as usize;
    let index = ri.min(LUT_RES - 1) * LUT_RES * LUT_RES
        + gi.min(LUT_RES - 1) * LUT_RES
        + bi.min(LUT_RES - 1);
    MATERIAL_LUT[index]
}

/// Brute-force O(1024) linear scan. Used to build the LUT and for test validation.
fn closest_material_bruteforce(color: [f32; 3]) -> MaterialId {
    let mut best_idx = 1u16;
    let mut best_dist = f32::MAX;
    let mut i = 1usize;
    while i < MATERIAL_PALETTE_SIZE {
        let c = MATERIAL_COLORS[i];
        let dr = color[0] - c[0];
        let dg = color[1] - c[1];
        let db = color[2] - c[2];
        let dist = dr * dr + dg * dg + db * db;
        if dist < best_dist {
            best_dist = dist;
            best_idx = i as MaterialId;
        }
        i += 1;
    }
    best_idx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
        let dr = a[0] - b[0];
        let dg = a[1] - b[1];
        let db = a[2] - b[2];
        dr * dr + dg * dg + db * db
    }

    /// The LUT result should pick a palette entry whose color distance to the
    /// input is at most slightly worse than the true nearest. The max additional
    /// error from 32^3 quantization is small enough to be visually imperceptible.
    const MAX_EXTRA_DIST_SQ: f32 = 0.005;

    #[test]
    fn lut_close_to_bruteforce_for_palette_colors() {
        for i in 1..MATERIAL_PALETTE_SIZE {
            let color = MATERIAL_COLORS[i];
            let lut_result = closest_material(color);
            let brute_result = closest_material_bruteforce(color);
            let lut_dist = color_distance_sq(color, MATERIAL_COLORS[lut_result as usize]);
            let brute_dist = color_distance_sq(color, MATERIAL_COLORS[brute_result as usize]);
            assert!(
                lut_dist <= brute_dist + MAX_EXTRA_DIST_SQ,
                "palette {i}: color={color:?}, lut={lut_result} (dist={lut_dist:.5}), brute={brute_result} (dist={brute_dist:.5})"
            );
        }
    }

    #[test]
    fn lut_close_to_bruteforce_for_sampled_colors() {
        for ri in 0..16 {
            for gi in 0..16 {
                for bi in 0..16 {
                    let color = [ri as f32 / 15.0, gi as f32 / 15.0, bi as f32 / 15.0];
                    let lut_result = closest_material(color);
                    let brute_result = closest_material_bruteforce(color);
                    let lut_dist = color_distance_sq(color, MATERIAL_COLORS[lut_result as usize]);
                    let brute_dist =
                        color_distance_sq(color, MATERIAL_COLORS[brute_result as usize]);
                    assert!(
                        lut_dist <= brute_dist + MAX_EXTRA_DIST_SQ,
                        "color={color:?}, lut={lut_result} (dist={lut_dist:.5}), brute={brute_result} (dist={brute_dist:.5})"
                    );
                }
            }
        }
    }
}
