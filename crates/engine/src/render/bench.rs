#![allow(clippy::expect_used)]

use super::{Renderer, Target};
use crate::{
    player::Camera,
    scene::{Scene, SceneSettings, SimulationSettings, VisualSettings},
    ui::{Canvas, Rect},
    world::{
        Material, MaterialId, Owner, OwnerId, StructureId, Support, Voxel, VoxelBounds,
        WorldSettings,
    },
};
use criterion::{BenchmarkId, Criterion};
use glam::{IVec3, Vec2, Vec3};
use std::{hint::black_box, sync::mpsc, time::Duration};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

#[cfg(target_os = "macos")]
const SIZE: [u32; 2] = [1280, 832];

#[cfg(not(target_os = "macos"))]
const SIZE: [u32; 2] = [2560, 1440];

const BATCH: u32 = 32;

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
pub(in crate::render) enum Layer {
    World,
    Ui,
    Overlay,
    Combined,
}

impl Layer {
    fn name(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Ui => "ui",
            Self::Overlay => "selection-outline",
            Self::Combined => "combined",
        }
    }
}

pub(in crate::render) struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
}

impl Gpu {
    fn new() -> Self {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());

        let adapter = pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None))
            .expect("GPU benchmarks require an available adapter; select with WGPU_BACKEND and WGPU_ADAPTER_NAME");

        let features =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;

        assert!(
            adapter.features().contains(features),
            "GPU benchmarks require TIMESTAMP_QUERY and TIMESTAMP_QUERY_INSIDE_ENCODERS; CPU timing is not a substitute"
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("pipeline benchmarks"),
            required_features: features,
            ..Default::default()
        }))
        .expect("create benchmark device");

        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("benchmark timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: BATCH * 2,
        });

        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamp resolve"),
            size: u64::from(BATCH) * 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamp readback"),
            size: u64::from(BATCH) * 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        assert!(
            queue.get_timestamp_period() > 0.0,
            "adapter must expose a timestamp period"
        );

        Self {
            device,
            queue,
            queries,
            resolve,
            readback,
        }
    }

    fn wait(&self) {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(TIMEOUT),
            })
            .expect("benchmark GPU completion timed out or failed");
    }

    fn encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pipeline benchmark"),
            })
    }

    fn warm_up(&self, workload: &Workload, layer: Layer) {
        for _ in 0..8 {
            let mut encoder = self.encoder();

            workload.encode(&mut encoder, layer);

            self.queue.submit([encoder.finish()]);
        }

        self.wait();
    }

    fn measure(&self, workload: &Workload, layer: Layer, iterations: u64) -> Duration {
        let mut remaining = iterations;

        let mut ticks = 0_u128;

        while remaining > 0 {
            let count = remaining.min(u64::from(BATCH)) as u32;

            let mut commands = Vec::with_capacity(count as usize + 1);

            for index in 0..count {
                let mut encoder = self.encoder();

                encoder.write_timestamp(&self.queries, index * 2);

                workload.encode(&mut encoder, layer);

                encoder.write_timestamp(&self.queries, index * 2 + 1);

                commands.push(encoder.finish());
            }

            let bytes = u64::from(count) * 16;

            let mut encoder = self.encoder();

            encoder.resolve_query_set(&self.queries, 0..count * 2, &self.resolve, 0);

            encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, bytes);

            commands.push(encoder.finish());

            self.queue.submit(commands);

            let slice = self.readback.slice(..bytes);

            let (sender, receiver) = mpsc::sync_channel(1);

            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });

            self.wait();

            receiver
                .recv_timeout(TIMEOUT)
                .expect("timestamp mapping callback missing")
                .expect("timestamp mapping failed");

            let mapped = slice.get_mapped_range().expect("mapped timestamp range");

            for pair in mapped.chunks_exact(16) {
                let start = u64::from_ne_bytes(pair[..8].try_into().expect("timestamp width"));

                let end = u64::from_ne_bytes(pair[8..].try_into().expect("timestamp width"));

                ticks += u128::from(end.wrapping_sub(start));
            }

            drop(mapped);

            self.readback.unmap();

            remaining -= u64::from(count);
        }

        let seconds = ticks as f64 * f64::from(self.queue.get_timestamp_period()) * 1e-9;

        assert!(
            seconds > 0.0 && seconds.is_finite(),
            "GPU timestamp result must be nonzero and finite"
        );

        Duration::from_secs_f64(seconds)
    }
}

struct Workload {
    renderer: Renderer,
    view: wgpu::TextureView,
}

impl Workload {
    fn new(gpu: &Gpu, size: [u32; 2], objects: i32) -> Self {
        let target = Target {
            device: &gpu.device,
            queue: &gpu.queue,
            format: FORMAT,
            size,
        };

        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen benchmark color"),
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

        let view = texture.create_view(&Default::default());

        let scene = scene(objects);

        let camera = Camera {
            position: Vec3::new(3.2, 4.0, 9.0),
            direction: (Vec3::new(3.2, 0.4, 3.2) - Vec3::new(3.2, 4.0, 9.0)).normalize(),
            fov_radians: 70_f32.to_radians(),
            near_plane: 0.03,
        };

        let selection = scene
            .raycast(Vec3::new(0.35, 2.0, 0.35), -Vec3::Y, 4.0)
            .expect("valid selection ray");

        assert!(
            selection.is_some(),
            "overlay benchmark must have a selected voxel"
        );

        let canvas = Canvas::new();

        for index in 0..objects * 8 {
            let position = Vec2::new((index % 32) as f32 * 28.0, (index / 32) as f32 * 18.0);

            canvas.rect(
                Rect::new(position, Vec2::new(40.0, 24.0)),
                [0.2, 0.6, 0.9, 0.65],
            );
        }

        let mut renderer = Renderer::for_target(target);

        renderer
            .prepare_target(target, &scene, camera, selection, &canvas, 1.0)
            .expect("prepare representative renderer workload");

        gpu.queue.submit([]);

        gpu.wait();

        Self { renderer, view }
    }

    fn recreate(&mut self, gpu: &Gpu, layer: Layer) {
        if matches!(layer, Layer::World | Layer::Combined) {
            self.renderer
                .world
                .reconfigure(&gpu.device, FORMAT, &self.renderer.view_layout);
        }

        if matches!(layer, Layer::Overlay | Layer::Combined) {
            self.renderer
                .overlay
                .reconfigure(&gpu.device, FORMAT, &self.renderer.view_layout);
        }

        if matches!(layer, Layer::Ui | Layer::Combined) {
            self.renderer.ui.reconfigure(&gpu.device, FORMAT);
        }
    }

    fn encode(&self, encoder: &mut wgpu::CommandEncoder, layer: Layer) {
        if matches!(layer, Layer::Combined) {
            self.renderer.draw_target(&self.view, encoder);

            return;
        }

        if matches!(layer, Layer::World) {
            self.renderer.world.draw_shadows(encoder);
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(layer.name()),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.renderer.clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: if matches!(layer, Layer::Ui) {
                None
            } else {
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.renderer.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                })
            },
            ..Default::default()
        });

        match layer {
            Layer::World => self.renderer.world.draw(&mut pass, &self.renderer.binding),
            Layer::Overlay => self
                .renderer
                .overlay
                .draw(&mut pass, &self.renderer.binding),
            Layer::Ui => self.renderer.ui.draw(&mut pass),
            Layer::Combined => unreachable!(),
        }
    }
}

fn scene(objects: i32) -> Scene {
    let mut scene = Scene::new(SceneSettings {
        world: WorldSettings {
            bounds: VoxelBounds {
                min: IVec3::ZERO,
                max: IVec3::splat(128),
            },
            materials: vec![Material {
                id: MaterialId(1),
                color: [0.6, 0.4, 0.2],
                density: 1.0,
            }],
            owners: vec![Owner {
                id: OwnerId(1),
                structure: StructureId(1),
                support: Support::Fixed,
            }],
            max_leaves: 4096,
            max_edit_voxels: 65536,
            max_support_voxels: 65536,
        },
        visuals: VisualSettings::default(),
        simulation: SimulationSettings::default(),
        render_leaf_edge: 8,
        max_mesh_vertices: 1_000_000,
        max_bodies: 128,
    })
    .expect("valid GPU scene settings");

    let mut transaction = scene.transaction();

    for index in 0..objects {
        let origin = IVec3::new(index % 8 * 8, 0, index / 8 * 8);

        for z in 2..6 {
            for y in 0..4 {
                for x in 2..6 {
                    transaction
                        .place(
                            origin + IVec3::new(x, y, z),
                            Voxel::new(MaterialId(1), OwnerId(1)),
                        )
                        .expect("valid GPU scene voxel");
                }
            }
        }
    }

    scene.commit(transaction).expect("valid GPU scene commit");

    scene
}

pub(in crate::render) fn register(
    criterion: &mut Criterion,
    gpu: &Gpu,
    layer: Layer,
    objects: &[i32],
) {
    let mut group = criterion.benchmark_group(layer.name());

    for &objects in objects {
        let workload_name = match layer {
            Layer::World => format!("{objects}-voxel-blocks"),
            Layer::Ui => format!("{}-alpha-rects", objects * 8),
            Layer::Overlay => "12-lines".to_owned(),
            Layer::Combined => format!("{objects}-blocks+{}-rects+outline", objects * 8),
        };

        let label = format!("{workload_name}/{}x{}", SIZE[0], SIZE[1]);

        let mut workload = Workload::new(gpu, SIZE, objects);

        workload.recreate(gpu, layer);

        gpu.wait();

        group.bench_function(
            BenchmarkId::new("cpu_pipeline_recreation_warm", &label),
            |bencher| {
                bencher.iter(|| workload.recreate(gpu, layer));
            },
        );

        gpu.wait();

        gpu.warm_up(&workload, layer);

        group.bench_function(BenchmarkId::new("cpu_encoding", &label), |bencher| {
            bencher.iter(|| {
                let mut encoder = gpu.encoder();

                workload.encode(&mut encoder, layer);

                black_box(encoder.finish());
            });
        });

        group.bench_function(BenchmarkId::new("gpu_execution", &label), |bencher| {
            bencher.iter_custom(|iterations| gpu.measure(&workload, layer, iterations));
        });

        gpu.wait();
    }

    group.finish();
}

pub fn run() {
    let mut criterion = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .configure_from_args();

    let gpu = Gpu::new();

    crate::render::world::bench::register(&mut criterion, &gpu);

    crate::render::ui::bench::register(&mut criterion, &gpu);

    crate::render::overlay::bench::register(&mut criterion, &gpu);

    register(&mut criterion, &gpu, Layer::Combined, &[16, 64]);

    gpu.wait();

    criterion.final_summary();
}
