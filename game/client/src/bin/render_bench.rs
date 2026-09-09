use capy_client::city::MeasurementOptions;
use capy_client::city::{City, CityConfig};
use clap::Parser;
use engine::measurement::{CapturePaths, MeasurementConfig, measure};

fn main() -> anyhow::Result<()> {
    let config = MeasurementOptions::parse();

    let start = std::time::Instant::now();

    let mut city_config = CityConfig {
        blocks: config.blocks,
        leaf_edge: config.leaf_edge,
        ..Default::default()
    };

    if config.opaque_parity {
        city_config.visuals.ambient = 1.0;
        city_config.visuals.ambient_occlusion = false;
        city_config.visuals.material_variation = false;
        city_config.visuals.shadows = None;
        city_config.visuals.sky = [0.0; 3];
        city_config.visuals.fog_distance = 1e20;
    }

    let mut city = City::new(city_config)?;

    let build_ms = start.elapsed().as_secs_f64() * 1000.0;

    if config.inside {
        city.camera.position = glam::Vec3::new(12.0, 3.0, 14.0);
    }

    let mut instances = city.instances.clone();

    let mut last_camera = city.camera;

    let camera = city.camera;

    let report = measure(
        &mut city.scene,
        camera,
        MeasurementConfig {
            width: config.width,
            height: config.height,
            warmup: config.warmup,
            frames: config.frames,
            capture: config.capture.map(|path| CapturePaths {
                depth_f32_le: path.with_extension("depth"),
                camera_columns_eye_f32_le: path.with_extension("camera"),
                rgba8_srgb: path,
            }),
            gpu_timestamps: !config.no_timestamps,
        },
        |frame, scene, view| {
            let frame = config.pose_frame.unwrap_or(frame.absolute_frame);

            if config.orbit {
                let angle = frame as f32 * 0.003;

                view.position = glam::Vec3::new(angle.sin() * 110.0, 45.0, -angle.cos() * 110.0);
                view.direction = (glam::Vec3::new(16.0, 20.0, 16.0) - view.position).normalize();
            }

            if config.moving {
                let index = (instances.len() / 2) | 1;

                if let Some(instance) = instances.get_mut(index) {
                    instance.rotation =
                        engine::Quat::from_rotation_y((frame as f32 * 0.01).sin() * 0.2);
                }

                scene.replace_render_instances(&instances)?;
            }

            last_camera = *view;

            Ok(())
        },
    )?;

    city.instances = instances;
    city.camera = last_camera;

    if let Some(path) = config.legacy_snapshot {
        city.save_legacy_snapshot(&path)?;
    }

    let output = serde_json::json!({ "opaque_parity": config.opaque_parity, "build_ms": build_ms, "blocks": config.blocks, "leaf_edge": config.leaf_edge, "moving": config.moving, "orbit": config.orbit, "inside": config.inside, "report": report });

    std::fs::write(config.output, serde_json::to_vec_pretty(&output)?)?;

    Ok(())
}
