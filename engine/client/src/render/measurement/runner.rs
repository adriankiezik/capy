use super::capture::{capture, read};
use super::{FrameIndex, MeasurementConfig, MeasurementFrame, MeasurementReport, Result};
use crate::{
    camera::Camera,
    render::renderer::{Renderer, Target},
    replica::Replica,
    ui::Canvas,
};
use std::time::Instant;

pub fn measure(
    scene: &mut Replica,
    mut camera: Camera,
    config: MeasurementConfig,
    mut update: impl FnMut(FrameIndex, &mut Replica, &mut Camera) -> Result<()>,
) -> Result<MeasurementReport> {
    anyhow::ensure!(
        config.width > 0
            && config.height > 0
            && config.frames > 0
            && config.warmup.checked_add(config.frames).is_some(),
        "invalid measurement dimensions or frame count"
    );

    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))?;

    let info = adapter.get_info();

    let features =
        wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;

    let timestamps = config.gpu_timestamps && adapter.features().contains(features);

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: if timestamps {
            features
        } else {
            wgpu::Features::empty()
        },
        ..Default::default()
    }))?;

    let errors = device.push_error_scope(wgpu::ErrorFilter::Validation);

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("measurement color"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let view = texture.create_view(&Default::default());

    let target = || Target {
        device: &device,
        queue: &queue,
        format,
        size: [config.width, config.height],
    };

    let mut renderer = Renderer::for_target(target());

    let canvas = Canvas::new();

    let queries = timestamps.then(|| {
        device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("measurement timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        })
    });

    let resolve = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("timestamp resolve"),
        size: 16,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("timestamp readback"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut samples = Vec::with_capacity(config.frames as usize);

    for frame in 0..config.warmup + config.frames {
        let start = Instant::now();

        update(
            FrameIndex {
                absolute_frame: frame,
                measured_frame: frame.checked_sub(config.warmup),
            },
            scene,
            &mut camera,
        )?;

        let prepared = Instant::now();

        renderer.prepare_target(target(), scene, camera, None, &canvas, 1.0)?;

        let encoded = Instant::now();

        let mut encoder = device.create_command_encoder(&Default::default());

        if let Some(queries) = &queries {
            encoder.write_timestamp(queries, 0);
        }

        renderer.draw_target(&view, &mut encoder);

        if let Some(queries) = &queries {
            encoder.write_timestamp(queries, 1);

            encoder.resolve_query_set(queries, 0..2, &resolve, 0);

            encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, 16);
        }

        let submission = queue.submit([encoder.finish()]);

        let submitted = Instant::now();

        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(60)),
        })?;

        let completed = Instant::now();

        let gpu_ms = if timestamps {
            let bytes = read(&device, &readback)?;

            let first = u64::from_le_bytes(bytes[0..8].try_into()?);

            let last = u64::from_le_bytes(bytes[8..16].try_into()?);

            anyhow::ensure!(last >= first, "invalid GPU timestamps");

            Some((last - first) as f64 * queue.get_timestamp_period() as f64 / 1_000_000.0)
        } else {
            None
        };

        if frame >= config.warmup {
            samples.push(MeasurementFrame {
                absolute_frame: frame,
                measured_frame: frame - config.warmup,
                update_ms: prepared.duration_since(start).as_secs_f64() * 1000.0,
                prepare_ms: encoded.duration_since(prepared).as_secs_f64() * 1000.0,
                encode_ms: submitted.duration_since(encoded).as_secs_f64() * 1000.0,
                wall_ms: completed.duration_since(start).as_secs_f64() * 1000.0,
                gpu_ms,
            });
        }
    }

    let (capture_hash, depth_hash) = capture(
        &device,
        &queue,
        &texture,
        renderer.depth_target(),
        camera,
        scene.settings.visuals.view_distance,
        config.capture.as_ref(),
    )?;

    if let Some(error) = pollster::block_on(errors.pop()) {
        return Err(error.into());
    }

    let geometry = scene.render_geometry()?;

    let geometry_bytes = renderer.geometry_bytes();

    Ok(MeasurementReport {
        adapter: info.name,
        backend: format!("{:?}", info.backend),
        driver: info.driver_info,
        width: config.width,
        height: config.height,
        warmup: config.warmup,
        capture_hash,
        depth_hash,
        geometry_bytes,
        mesh_instance_count: geometry.len(),
        samples,
    })
}
