#![allow(clippy::unwrap_used)]

use super::cascades::{COUNT, Cascades};
use crate::{Aabb, player::Camera, scene::ShadowSettings};
use glam::Vec3;

#[test]
fn cascades_contain_receiver_slices_and_offscreen_caster_depth_at_camera_extremes() {
    for aspect in [0.25, 1.0, 4.0] {
        for fov in [20f32, 70.0, 140.0] {
            for near in [0.01, 1.0] {
                for sun in [Vec3::Y, -Vec3::Y, Vec3::new(1.0, 0.01, 0.0).normalize()] {
                    let camera = Camera {
                        position: Vec3::new(-48.0, 20.0, 32.0),
                        direction: Vec3::new(0.2, -0.7, -1.0).normalize(),
                        fov_radians: fov.to_radians(),
                        near_plane: near,
                    };

                    let bounds = Aabb {
                        min: Vec3::splat(-128.0),
                        max: Vec3::splat(128.0),
                    };

                    let cascades = Cascades::new(
                        camera.matrix(aspect, 160.0).unwrap(),
                        camera.position,
                        sun,
                        Some(bounds),
                        ShadowSettings {
                            resolution: 256,
                            distance: 96.0,
                        },
                    );

                    assert!(cascades.splits[..COUNT].windows(2).all(|s| s[0] < s[1]));

                    let right = camera.direction.cross(Vec3::Y).normalize();

                    let up = right.cross(camera.direction);

                    for (i, matrix) in cascades.matrices.iter().enumerate() {
                        assert!(matrix.is_finite());

                        assert!(matrix.determinant().abs() > 0.0);

                        let start = if i == 0 {
                            near
                        } else {
                            (cascades.splits[i - 1] * 0.9).max(near)
                        };

                        for depth in [start, cascades.splits[i]] {
                            let half_height = depth * (camera.fov_radians * 0.5).tan();

                            for x in [-1.0, 1.0] {
                                for y in [-1.0, 1.0] {
                                    let point = camera.position
                                        + camera.direction * depth
                                        + right * (x * half_height * aspect)
                                        + up * (y * half_height);

                                    let clip = matrix.project_point3(point);

                                    assert!(
                                        clip.x.abs() <= 1.001
                                            && clip.y.abs() <= 1.001
                                            && (-0.001..=1.001).contains(&clip.z),
                                        "cascade {i}, aspect {aspect}, fov {fov}, near {near}: {clip:?}"
                                    );
                                }
                            }
                        }

                        for x in [-128.0, 128.0] {
                            for y in [-128.0, 128.0] {
                                for z in [-128.0, 128.0] {
                                    let depth = matrix.project_point3(Vec3::new(x, y, z)).z;

                                    assert!(
                                        (0.0..=1.0).contains(&depth),
                                        "offscreen caster clipped in cascade {i}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
