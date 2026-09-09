use crate::{Aabb, replica::ShadowSettings};
use glam::{Mat4, Vec3};

pub(super) const COUNT: usize = 3;

pub(super) struct Cascades {
    pub(super) matrices: [Mat4; COUNT],
    pub(super) splits: [f32; 4],
    pub(super) direction: Vec3,
}

impl Cascades {
    pub(super) fn new(
        matrix: Mat4,
        eye: Vec3,
        sun: Vec3,
        bounds: Option<Aabb>,
        settings: ShadowSettings,
    ) -> Self {
        let inverse = matrix.inverse();

        let near: [Vec3; 4] = std::array::from_fn(|i| {
            inverse.project_point3(Vec3::new(
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                0.0,
            ))
        });

        let far: [Vec3; 4] = std::array::from_fn(|i| {
            inverse.project_point3(Vec3::new(
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                1.0,
            ))
        });

        let near_center = near.iter().sum::<Vec3>() * 0.25;

        let far_center = far.iter().sum::<Vec3>() * 0.25;

        let direction = (far_center - near_center).normalize();

        let span = near_center.distance(far_center);

        let near_depth = (near_center - eye).dot(direction);

        let distance = settings.distance.min(near_depth + span);

        let range = distance - near_depth;

        let splits = [
            near_depth + range * 0.1,
            near_depth + range * 0.32,
            distance,
            0.0,
        ];

        let up = if sun.dot(Vec3::Y).abs() > 0.95 {
            Vec3::Z
        } else {
            Vec3::Y
        };

        let view = glam::camera::rh::view::look_at_mat4(Vec3::ZERO, -sun, up);

        let matrices = std::array::from_fn(|cascade| {
            let start = if cascade == 0 {
                0.0
            } else {
                ((splits[cascade - 1] * 0.9 - near_depth) / span).max(0.0)
            };

            let end = (splits[cascade] - near_depth) / span;

            let corners: [Vec3; 8] = std::array::from_fn(|i| {
                near[i % 4].lerp(far[i % 4], if i < 4 { start } else { end })
            });

            let center = corners.iter().sum::<Vec3>() / 8.0;

            let radius = (corners
                .iter()
                .map(|p| p.distance(center))
                .fold(0.0_f32, f32::max)
                * 16.0)
                .ceil()
                / 16.0;

            let radius = radius * settings.resolution as f32 / (settings.resolution - 2) as f32;

            let texel = 2.0 * radius / settings.resolution as f32;

            let center = view.transform_point3(center);

            let x = (center.x / texel).round() * texel;

            let y = (center.y / texel).round() * texel;

            let mut min_z = center.z - radius;

            let mut max_z = center.z + radius;

            if let Some(bounds) = bounds {
                for i in 0..8 {
                    let p = Vec3::new(
                        if i & 1 == 0 {
                            bounds.min.x
                        } else {
                            bounds.max.x
                        },
                        if i & 2 == 0 {
                            bounds.min.y
                        } else {
                            bounds.max.y
                        },
                        if i & 4 == 0 {
                            bounds.min.z
                        } else {
                            bounds.max.z
                        },
                    );

                    let z = view.transform_point3(p).z;

                    min_z = min_z.min(z);
                    max_z = max_z.max(z);
                }
            }

            glam::camera::rh::proj::directx::orthographic(
                x - radius,
                x + radius,
                y - radius,
                y + radius,
                -max_z - 1.0,
                -min_z + 1.0,
            ) * view
        });

        Self {
            matrices,
            splits,
            direction,
        }
    }
}
