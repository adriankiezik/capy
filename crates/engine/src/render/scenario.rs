use super::{Renderer, renderer::Target};
use crate::{View, ui::Canvas};
use anyhow::{Context, Result, ensure};
use std::{
    collections::VecDeque,
    sync::mpsc,
    time::{Duration, Instant},
};

const FRAMES_IN_FLIGHT: usize = 3;

const TIMEOUT: Duration = Duration::from_secs(30);

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct Slot {
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
}

pub struct ScenarioTimings {
    pub prepare: Duration,
    pub encode: Duration,
    pub backpressure: Duration,
}

pub struct ScenarioRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    view: wgpu::TextureView,
    size: [u32; 2],
    adapter: wgpu::AdapterInfo,
    queries: Option<wgpu::QuerySet>,
    slots: Vec<Slot>,
    pending: VecDeque<(wgpu::SubmissionIndex, usize)>,
    next_slot: usize,
    gpu_times: Vec<Duration>,
}

impl ScenarioRenderer {
    pub fn new(size: [u32; 2]) -> Result<Self> {
        ensure!(
            size.iter().all(|&value| value > 0),
            "resolution must be positive"
        );

        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());

        let adapter = pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(
            &instance, None,
        ))
        .context("no GPU adapter available; select using WGPU_BACKEND and WGPU_ADAPTER_NAME")?;

        let timestamp_features =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;

        let timestamps = adapter.features().contains(timestamp_features);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("scenario benchmark"),
            required_features: if timestamps {
                timestamp_features
            } else {
                wgpu::Features::empty()
            },
            ..Default::default()
        }))
        .context("create scenario GPU device")?;

        ensure!(
            size.iter()
                .all(|&value| value <= device.limits().max_texture_dimension_2d),
            "resolution exceeds device limits"
        );

        let queries = timestamps.then(|| {
            device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("scenario timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: FRAMES_IN_FLIGHT as u32 * 2,
            })
        });

        let slots = (0..if timestamps { FRAMES_IN_FLIGHT } else { 0 })
            .map(|_| Slot {
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scenario timestamp resolve"),
                    size: 16,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scenario timestamp readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            })
            .collect();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scenario offscreen target"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let renderer = Renderer::for_target(Target {
            device: &device,
            queue: &queue,
            format: FORMAT,
            size,
        });

        let view = texture.create_view(&Default::default());

        Ok(Self {
            device,
            queue,
            renderer,
            view,
            size,
            adapter: adapter.get_info(),
            queries,
            slots,
            pending: VecDeque::new(),
            next_slot: 0,
            gpu_times: Vec::new(),
        })
    }

    pub fn adapter(&self) -> &wgpu::AdapterInfo {
        &self.adapter
    }

    pub fn timestamps_supported(&self) -> bool {
        self.queries.is_some()
    }

    pub fn frames_in_flight(&self) -> usize {
        FRAMES_IN_FLIGHT
    }

    fn complete_oldest(&mut self) -> Result<()> {
        let Some((submission, index)) = self.pending.pop_front() else {
            return Ok(());
        };

        if self.queries.is_some() {
            let slot = &self.slots[index];

            let (sender, receiver) = mpsc::sync_channel(1);

            slot.readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sender.send(result);
                });

            self.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(TIMEOUT),
                })
                .context("GPU completion failed")?;

            receiver
                .recv_timeout(TIMEOUT)
                .context("timestamp mapping callback timed out")??;

            let mapped = slot.readback.slice(..).get_mapped_range()?;

            let start = u64::from_ne_bytes(mapped[..8].try_into()?);

            let end = u64::from_ne_bytes(mapped[8..].try_into()?);

            let seconds = end.wrapping_sub(start) as f64
                * f64::from(self.queue.get_timestamp_period())
                * 1e-9;

            ensure!(
                seconds.is_finite() && seconds > 0.0,
                "invalid GPU timestamp duration"
            );

            self.gpu_times.push(Duration::from_secs_f64(seconds));

            drop(mapped);

            slot.readback.unmap();
        } else {
            self.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(TIMEOUT),
                })
                .context("GPU completion failed")?;
        }

        Ok(())
    }

    pub fn submit(&mut self, view: View<'_>, canvas: &Canvas) -> Result<ScenarioTimings> {
        let start = Instant::now();

        if self.pending.len() == FRAMES_IN_FLIGHT {
            self.complete_oldest()?;
        }

        let backpressure = start.elapsed();

        let start = Instant::now();

        self.renderer.prepare_target(
            Target {
                device: &self.device,
                queue: &self.queue,
                format: FORMAT,
                size: self.size,
            },
            view.scene,
            view.camera,
            view.selection,
            canvas,
            1.0,
        )?;

        let prepare = start.elapsed();

        let start = Instant::now();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scenario frame"),
            });

        let index = self.next_slot;

        if let Some(queries) = &self.queries {
            encoder.write_timestamp(queries, index as u32 * 2);
        }

        self.renderer.draw_target(&self.view, &mut encoder);

        if let Some(queries) = &self.queries {
            encoder.write_timestamp(queries, index as u32 * 2 + 1);

            let slot = &self.slots[index];

            encoder.resolve_query_set(
                queries,
                index as u32 * 2..index as u32 * 2 + 2,
                &slot.resolve,
                0,
            );

            encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, 16);
        }

        let submission = self.queue.submit([encoder.finish()]);

        self.pending.push_back((submission, index));

        self.next_slot = (index + 1) % FRAMES_IN_FLIGHT;

        Ok(ScenarioTimings {
            prepare,
            encode: start.elapsed(),
            backpressure,
        })
    }

    pub fn finish(&mut self) -> Result<Vec<Duration>> {
        while !self.pending.is_empty() {
            self.complete_oldest()?;
        }

        Ok(std::mem::take(&mut self.gpu_times))
    }
}
