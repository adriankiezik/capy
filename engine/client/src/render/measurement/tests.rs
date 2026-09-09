#![allow(clippy::unwrap_used)]
use super::{CapturePaths, MeasurementConfig, MeasurementReport, measure};
use crate::{
    camera::Camera,
    replica::{ModelConfig, Replica, ReplicaConfig, ShadowSettings, VoxelInstance, VoxelModel},
};
use glam::{IVec3, Quat, Vec3};

fn scene(instances: &[VoxelInstance]) -> Replica {
    let mut scene = Replica::empty(ReplicaConfig::default()).unwrap();

    scene.settings.visuals.shadows = Some(ShadowSettings {
        resolution: 256,
        distance: 20.0,
    });

    scene.replace_render_instances(instances).unwrap();

    scene
}

fn opaque_scene(instances: &[VoxelInstance]) -> Replica {
    let mut replica = scene(instances);

    replica.settings.visuals.ambient = 1.0;
    replica.settings.visuals.ambient_occlusion = false;
    replica.settings.visuals.material_variation = false;
    replica.settings.visuals.shadows = None;
    replica.settings.visuals.sky = [0.0; 3];
    replica.settings.visuals.fog_distance = 1e20;

    replica
}

fn camera() -> Camera {
    Camera {
        position: Vec3::new(3.7, 2.8, -4.3),
        direction: (Vec3::new(0.7, 0.4, 0.3) - Vec3::new(3.7, 2.8, -4.3)).normalize(),
        fov_radians: 1.0,
        near_plane: 0.05,
    }
}

fn model(alternate: bool, leaf_edge: i32) -> std::sync::Arc<VoxelModel> {
    VoxelModel::build(
        IVec3::splat(18),
        &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        |p| {
            if p.y == 0 || p.x == 0 || p.z == 17 || (alternate && p.y < 8 && p.x < 8) {
                Some((p.y % 2) as usize)
            } else {
                None
            }
        },
        ModelConfig {
            leaf_edge,
            ..Default::default()
        },
    )
    .unwrap()
}

fn capture(
    initial: &[VoxelInstance],
    final_instances: &[VoxelInstance],
    switch: u32,
) -> MeasurementReport {
    measure(
        &mut scene(initial),
        camera(),
        MeasurementConfig {
            width: 160,
            height: 120,
            warmup: 0,
            frames: 5,
            gpu_timestamps: false,
            ..Default::default()
        },
        |frame, scene, _| {
            if frame.absolute_frame == switch {
                scene.replace_render_instances(final_instances)?;
            }

            Ok(())
        },
    )
    .unwrap()
}

#[test]
fn retained_renderer_matches_fresh_after_scene_changes() {
    let shared = model(false, 8);

    let initial = vec![
        VoxelInstance {
            id: 1,
            model: shared.clone(),
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
        },
        VoxelInstance {
            id: 2,
            model: shared,
            translation: Vec3::new(-2.0, 0.0, 0.0),
            rotation: Quat::IDENTITY,
            scale: 1.0,
        },
    ];

    let mut moved = initial.clone();

    moved[0].translation += Vec3::new(0.3, 0.2, 0.4);
    moved[0].rotation = Quat::from_rotation_y(0.37);
    moved[0].scale = 0.8;

    let mut reordered = moved.clone();

    reordered.reverse();

    let mut replaced = initial.clone();

    replaced[0].model = model(true, 8);

    let mut outside = initial.clone();

    outside[0].translation = Vec3::splat(200.0);

    for final_instances in [
        moved,
        reordered,
        replaced,
        outside,
        initial[1..].to_vec(),
        Vec::new(),
    ] {
        let fresh = capture(&final_instances, &final_instances, 0);

        for switch in [3, 4] {
            let retained = capture(&initial, &final_instances, switch);

            assert_eq!(
                retained.capture_hash, fresh.capture_hash,
                "color mismatch after change at frame {switch}"
            );

            assert_eq!(
                retained.depth_hash, fresh.depth_hash,
                "depth mismatch after change at frame {switch}"
            );

            assert_eq!(retained.geometry_bytes, fresh.geometry_bytes);
        }
    }
}

#[test]
fn render_leaf_sizes_preserve_color_and_depth() {
    let root = std::env::temp_dir().join(format!("capy-leaf-parity-{}", std::process::id()));

    std::fs::create_dir_all(&root).unwrap();

    let mut reference: Option<(Vec<u8>, Vec<f32>)> = None;

    for edge in [8, 16, 32, 64] {
        let instances = [VoxelInstance {
            id: 1,
            model: model(false, edge),
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
        }];

        let path = root.join(format!("leaf-{edge}.rgba"));

        measure(
            &mut opaque_scene(&instances),
            camera(),
            MeasurementConfig {
                width: 160,
                height: 120,
                warmup: 2,
                frames: 1,
                gpu_timestamps: false,
                capture: Some(CapturePaths {
                    rgba8_srgb: path.clone(),
                    depth_f32_le: path.with_extension("depth"),
                    camera_columns_eye_f32_le: path.with_extension("camera"),
                }),
            },
            |_, _, _| Ok(()),
        )
        .unwrap();

        let color = std::fs::read(&path).unwrap();

        let depth: Vec<_> = std::fs::read(path.with_extension("depth"))
            .unwrap()
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();

        if let Some((expected_color, expected_depth)) = &reference {
            assert_eq!(
                &color, expected_color,
                "material mismatch for leaf edge {edge}"
            );

            for (pixel, (&actual, &expected)) in depth.iter().zip(expected_depth).enumerate() {
                assert!(
                    actual.is_finite() && (actual - expected).abs() <= 2e-6,
                    "depth mismatch at {pixel} for leaf edge {edge}: {actual} != {expected}"
                );
            }
        } else {
            reference = Some((color, depth));
        }
    }

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rendered_pixels_match_voxel_ray_oracle() {
    let root = std::env::temp_dir().join(format!("capy-ray-parity-{}", std::process::id()));

    std::fs::create_dir_all(&root).unwrap();

    let model = model(false, 8);

    let instances = [
        VoxelInstance {
            id: 1,
            model: model.clone(),
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
        },
        VoxelInstance {
            id: 2,
            model,
            translation: Vec3::new(-2.1, 0.3, 0.2),
            rotation: Quat::from_rotation_y(0.37),
            scale: 0.8,
        },
    ];

    for (case, camera) in [
        camera(),
        Camera {
            position: Vec3::new(0.57, 0.53, 0.42),
            direction: Vec3::new(-0.1, 0.2, 1.0).normalize(),
            ..camera()
        },
    ]
    .into_iter()
    .enumerate()
    {
        let path = root.join(format!("ray-{case}.rgba"));

        let scene = &mut opaque_scene(&instances);

        let matrix = camera
            .matrix(160.0 / 120.0, scene.settings.visuals.view_distance)
            .unwrap()
            .as_dmat4();

        let inverse = matrix.inverse();

        measure(
            scene,
            camera,
            MeasurementConfig {
                width: 160,
                height: 120,
                warmup: 2,
                frames: 1,
                gpu_timestamps: false,
                capture: Some(CapturePaths {
                    rgba8_srgb: path.clone(),
                    depth_f32_le: path.with_extension("depth"),
                    camera_columns_eye_f32_le: path.with_extension("camera"),
                }),
            },
            |_, _, _| Ok(()),
        )
        .unwrap();

        let pixels = std::fs::read(&path).unwrap();

        let depth: Vec<_> = std::fs::read(path.with_extension("depth"))
            .unwrap()
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();

        let mut checked = 0;

        let transforms: Vec<_> = instances
            .iter()
            .map(|instance| instance.matrix().as_dmat4().inverse())
            .collect();

        for y in 0..120 {
            for x in 0..160 {
                let ndc = glam::DVec3::new(
                    (x as f64 + 0.5) / 80.0 - 1.0,
                    1.0 - (y as f64 + 0.5) / 60.0,
                    0.0,
                );

                let start = inverse.project_point3(ndc);

                let end = inverse.project_point3(glam::DVec3::new(ndc.x, ndc.y, 1.0));

                let direction = end - start;

                let mut nearest = 1.0_f64;

                let mut color = [0, 0, 0, 255];

                let mut boundary = false;

                let mut hit_point = glam::DVec3::ZERO;

                for transform in &transforms {
                    let origin = transform.transform_point3(start);

                    let delta = transform.transform_vector3(direction);

                    for z in 0..18 {
                        for y in 0..18 {
                            for x in 0..18 {
                                if y != 0 && x != 0 && z != 17 {
                                    continue;
                                }

                                let min = glam::DVec3::new(x as f64, y as f64, z as f64) * 0.1;

                                let max = min + glam::DVec3::splat(0.1);

                                let a = (min - origin) / delta;

                                let b = (max - origin) / delta;

                                let enter = a.min(b).max_element();

                                let leave = a.max(b).min_element();

                                if enter < 0.0 || enter > leave || enter >= nearest {
                                    continue;
                                }

                                nearest = enter;
                                color = if y % 2 == 0 {
                                    [255, 0, 0, 255]
                                } else {
                                    [0, 255, 0, 255]
                                };

                                let point = (origin + delta * enter) * 10.0;

                                hit_point = point;

                                let pixel_width = (start + direction * enter
                                    - camera.position.as_dvec3())
                                .length()
                                    * (camera.fov_radians as f64 * 0.5).tan()
                                    / 60.0;

                                let tolerance =
                                    pixel_width * transform.x_axis.truncate().length() * 10.0
                                        / 128.0
                                        + 1e-5;

                                boundary = (point - point.round())
                                    .abs()
                                    .to_array()
                                    .into_iter()
                                    .filter(|distance| *distance < tolerance)
                                    .count()
                                    > 1;
                            }
                        }
                    }
                }

                if boundary {
                    continue;
                }

                let index = y * 160 + x;

                let expected_depth = if nearest == 1.0 {
                    1.0
                } else {
                    matrix.project_point3(start + direction * nearest).z as f32
                };

                assert_eq!(
                    &pixels[index * 4..index * 4 + 4],
                    &color,
                    "material at ({x}, {y}), view {case}, hit {hit_point:?}"
                );

                assert!(
                    (depth[index] - expected_depth).abs() <= 2e-6,
                    "depth at ({x}, {y}), view {case}: {} != {expected_depth}",
                    depth[index]
                );

                checked += 1;
            }
        }

        assert!(checked > 19000);
    }

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn camera_and_shadow_changes_match_fresh_rendering() {
    let instances = [VoxelInstance {
        id: 1,
        model: model(false, 8),
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: 1.0,
    }];

    let final_camera = Camera {
        position: Vec3::new(-2.5, 1.7, -2.0),
        direction: Vec3::new(3.0, -0.8, 3.0).normalize(),
        ..camera()
    };

    for shadows in [
        None,
        Some(ShadowSettings {
            resolution: 512,
            distance: 12.0,
        }),
    ] {
        let render = |switch| {
            measure(
                &mut scene(&instances),
                camera(),
                MeasurementConfig {
                    width: 160,
                    height: 120,
                    warmup: 0,
                    frames: 5,
                    gpu_timestamps: false,
                    ..Default::default()
                },
                |frame, scene, view| {
                    if frame.absolute_frame == switch {
                        *view = final_camera;
                        scene.settings.visuals.shadows = shadows;
                        scene.settings.visuals.sunlight = [-0.5, 0.7, 0.2];
                    }

                    Ok(())
                },
            )
            .unwrap()
        };

        let fresh = render(0);

        let retained = render(3);

        assert_eq!(retained.capture_hash, fresh.capture_hash);

        assert_eq!(retained.depth_hash, fresh.depth_hash);
    }
}

#[test]
fn shadow_distance_crossing_near_plane_matches_fresh_rendering() {
    let instances = [VoxelInstance {
        id: 1,
        model: model(false, 8),
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: 1.0,
    }];

    let render = |distances: &[f32]| {
        measure(
            &mut scene(&instances),
            Camera {
                near_plane: 2.0,
                ..camera()
            },
            MeasurementConfig {
                width: 160,
                height: 120,
                warmup: 0,
                frames: distances.len() as u32 * 3,
                gpu_timestamps: false,
                ..Default::default()
            },
            |frame, scene, _| {
                scene.settings.visuals.shadows = Some(ShadowSettings {
                    resolution: 256,
                    distance: distances[frame.absolute_frame as usize / 3],
                });

                Ok(())
            },
        )
        .unwrap()
    };

    let disabled = render(&[1.5]);

    let enabled = render(&[20.0]);

    assert_ne!(disabled.capture_hash, enabled.capture_hash);

    for distances in [vec![1.5, 20.0], vec![20.0, 1.5], vec![1.5, 20.0, 1.5, 20.0]] {
        let expected = if distances.last() == Some(&20.0) {
            &enabled
        } else {
            &disabled
        };

        let retained = render(&distances);

        assert_eq!(
            retained.capture_hash, expected.capture_hash,
            "shadow distance sequence {distances:?}"
        );

        assert_eq!(retained.depth_hash, expected.depth_hash);
    }
}

#[test]
fn measurement_reports_warmup_indices_and_writes_only_explicit_artifacts() {
    let root =
        std::env::temp_dir().join(format!("capy-measurement-contract-{}", std::process::id()));

    std::fs::create_dir_all(&root).unwrap();

    let paths = CapturePaths {
        rgba8_srgb: root.join("pixels.bin"),
        depth_f32_le: root.join("distance.bin"),
        camera_columns_eye_f32_le: root.join("view.bin"),
    };

    let mut replica = scene(&[]);

    replica.settings.visuals.ambient = 0.17;
    replica.settings.visuals.sky = [0.13, 0.23, 0.33];

    let mut indices = Vec::new();

    let report = measure(
        &mut replica,
        camera(),
        MeasurementConfig {
            width: 17,
            height: 11,
            warmup: 2,
            frames: 2,
            capture: Some(paths.clone()),
            gpu_timestamps: false,
        },
        |index, _, _| {
            indices.push(index);

            Ok(())
        },
    )
    .unwrap();

    assert_eq!(
        indices
            .iter()
            .map(|index| (index.absolute_frame, index.measured_frame))
            .collect::<Vec<_>>(),
        [(0, None), (1, None), (2, Some(0)), (3, Some(1))]
    );

    assert_eq!(
        report
            .samples
            .iter()
            .map(|sample| (sample.absolute_frame, sample.measured_frame))
            .collect::<Vec<_>>(),
        [(2, 0), (3, 1)]
    );

    assert_eq!(report.mesh_instance_count, 0);

    assert_eq!(replica.settings.visuals.ambient, 0.17);

    assert_eq!(replica.settings.visuals.sky, [0.13, 0.23, 0.33]);

    assert_eq!(std::fs::read(&paths.rgba8_srgb).unwrap().len(), 17 * 11 * 4);

    let depth = std::fs::read(&paths.depth_f32_le).unwrap();

    assert_eq!(depth.len(), 17 * 11 * 4);

    assert!(
        depth
            .chunks_exact(4)
            .all(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()) == 1.0)
    );

    let view = std::fs::read(&paths.camera_columns_eye_f32_le).unwrap();

    assert_eq!(view.len(), 76);

    let eye: Vec<_> = view[64..]
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();

    assert_eq!(eye, camera().position.to_array());

    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);

    std::fs::remove_dir_all(root).unwrap();
}
