#![allow(clippy::unwrap_used)]

use super::{Renderer, renderer::Target};
use crate::{
    player::Camera,
    scene::{
        Hit, Scene,
        tests::{place, settings},
    },
    ui::{Canvas, Rect, TextStyle},
    world::{MaterialId, OwnerId, Voxel, tests::voxel},
};
use glam::{IVec3, Vec2, Vec3};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(15);

const FORMATS: [wgpu::TextureFormat; 3] = [
    wgpu::TextureFormat::Rgba8UnormSrgb,
    wgpu::TextureFormat::Bgra8UnormSrgb,
    wgpu::TextureFormat::Rgba8Unorm,
];

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: wgpu::AdapterInfo,
    errors: mpsc::Receiver<String>,
}

impl Gpu {
    fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            flags: wgpu::InstanceFlags::VALIDATION,
            ..wgpu::InstanceDescriptor::new_without_display_handle().with_env()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            ..Default::default()
        }))
        .unwrap();

        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();

        let (sender, errors) = mpsc::channel();

        let lost = sender.clone();

        device.on_uncaptured_error(Arc::new(move |error| {
            let _ = sender.send(format!("uncaptured GPU error: {error}"));
        }));

        device.set_device_lost_callback(move |reason, message| {
            let _ = lost.send(format!("device lost: {reason:?}: {message}"));
        });

        Self {
            device,
            queue,
            adapter: adapter.get_info(),
            errors,
        }
    }

    fn wait(&self, label: &str) {
        let result = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(TIMEOUT),
        });

        assert!(
            result.is_ok(),
            "{label}: GPU completion failed on {:?}: {result:?}",
            self.adapter
        );
    }

    fn checked<T>(&self, label: &str, operation: impl FnOnce() -> T) -> T {
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);

        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);

        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);

        let result = operation();

        self.wait(label);

        for scope in [internal, memory, validation] {
            let error = pollster::block_on(scope.pop());

            assert!(
                error.is_none(),
                "{label}: GPU error on {:?}: {error:?}",
                self.adapter
            );
        }

        let errors: Vec<_> = self.errors.try_iter().collect();

        assert!(
            errors.is_empty(),
            "{label}: GPU errors on {:?}: {errors:?}",
            self.adapter
        );

        result
    }

    fn target(&self, fixture: &Fixture) -> Target<'_> {
        Target {
            device: &self.device,
            queue: &self.queue,
            format: fixture.format,
            size: fixture.size,
        }
    }

    fn renderer(&self, fixture: &Fixture) -> Renderer {
        self.checked("create renderer", || {
            Renderer::for_target(self.target(fixture))
        })
    }

    fn render(
        &self,
        label: &str,
        renderer: &mut Renderer,
        fixture: &Fixture,
        output: &mut Option<wgpu::Texture>,
    ) -> Vec<u8> {
        self.checked(label, || {
            renderer
                .prepare_target(
                    self.target(fixture),
                    &fixture.scene,
                    fixture.camera,
                    fixture.selection,
                    &fixture.canvas,
                    fixture.dpi,
                )
                .unwrap();

            let [width, height] = fixture.size;

            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };

            if !output
                .as_ref()
                .is_some_and(|texture| texture.size() == size && texture.format() == fixture.format)
            {
                *output = Some(self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: fixture.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                }));
            }

            let texture = output.as_ref().unwrap();

            let view = texture.create_view(&Default::default());

            let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
                * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("renderer test readback"),
                size: stride as u64 * height as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            let mut encoder = self.device.create_command_encoder(&Default::default());

            renderer.draw_target(&view, &mut encoder);

            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride),
                        rows_per_image: Some(height),
                    },
                },
                size,
            );

            self.queue.submit([encoder.finish()]);

            let (sender, receiver) = mpsc::channel();

            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sender.send(result);
                });

            self.wait(label);

            receiver.recv_timeout(TIMEOUT).unwrap().unwrap();

            let mapped = buffer.slice(..).get_mapped_range().unwrap();

            let pixels = mapped
                .chunks_exact(stride as usize)
                .flat_map(|row| row[..width as usize * 4].iter().copied())
                .collect();

            drop(mapped);

            buffer.unmap();

            pixels
        })
    }
}

struct Fixture {
    scene: Scene,
    canvas: Canvas,
    camera: Camera,
    selection: Option<Hit>,
    dpi: f32,
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

impl Fixture {
    fn new() -> Self {
        let mut config = settings();

        config.world.bounds.min.y = -1;

        Self {
            scene: Scene::new(config).unwrap(),
            canvas: Canvas::new(),
            camera: Camera {
                position: Vec3::new(2.5, 1.8, 4.0),
                direction: (Vec3::new(0.0, 0.25, 0.0) - Vec3::new(2.5, 1.8, 4.0)).normalize(),
                fov_radians: 60.0f32.to_radians(),
                near_plane: 0.03,
            },
            selection: None,
            dpi: 1.0,
            size: [137, 101],
            format: FORMATS[0],
        }
    }

    fn change(&mut self, stage: usize) {
        match stage {
            0 => {}
            1 => {
                place(
                    &mut self.scene,
                    (-4..4).flat_map(|x| {
                        (-1..4).flat_map(move |y| {
                            (-2..2).map(move |z| (IVec3::new(x, y, z), voxel(1)))
                        })
                    }),
                );

                self.selection = self
                    .scene
                    .raycast(self.camera.position, self.camera.direction, 10.0)
                    .unwrap();

                assert!(self.selection.is_some());

                self.canvas.rect(
                    Rect::new(Vec2::new(4.0, 4.0), Vec2::new(30.0, 12.0)),
                    [0.1, 0.8, 0.2, 0.5],
                );

                self.canvas
                    .text("GPU", Vec2::new(5.0, 20.0), &TextStyle::default());
            }
            2 => {
                let world = self.scene.snapshot();

                place(
                    &mut self.scene,
                    (-16..16)
                        .flat_map(|x| (-16..16).map(move |z| (IVec3::new(x, -1, z), voxel(1))))
                        .filter(|(p, _)| world.voxel(*p).unwrap().is_empty()),
                );

                let world = self.scene.snapshot();

                place(
                    &mut self.scene,
                    (0..24)
                        .flat_map(|i| {
                            (-1..=4).map(move |y| {
                                (IVec3::new((i % 8 - 4) * 8, y, (i / 8 - 1) * 8), voxel(3))
                            })
                        })
                        .filter(|(p, _)| world.voxel(*p).unwrap().is_empty()),
                );

                place(
                    &mut self.scene,
                    (0..3).flat_map(|x| (10..13).map(move |y| (IVec3::new(x, y, 0), voxel(2)))),
                );

                assert_eq!(self.scene.stats().bodies, 1);

                self.canvas = Canvas::new();

                for i in 0..80 {
                    self.canvas.rect(
                        Rect::new(
                            Vec2::new((i % 16) as f32 * 7.0, (i / 16) as f32 * 5.0),
                            Vec2::new(4.0, 3.0),
                        ),
                        [0.2, 0.4, 0.9, 0.5],
                    );
                }

                self.scene.settings.visuals.sky = [0.12, 0.18, 0.3];
                self.scene.settings.visuals.sunlight = [-1.0, 1.0, 0.0];
                self.scene.settings.visuals.fog_distance = 4.0;
                self.camera.position = Vec3::new(-2.5, 2.0, 3.0);
                self.camera.direction =
                    (Vec3::new(0.0, 0.5, 0.0) - self.camera.position).normalize();
                self.selection = None;
            }
            3 => {
                let mut transaction = self.scene.transaction();

                transaction.remove(IVec3::new(0, 3, 1)).unwrap();

                transaction
                    .replace(
                        IVec3::new(1, 3, 1),
                        voxel(1),
                        Voxel::new(MaterialId(2), OwnerId(1)),
                    )
                    .unwrap();

                self.scene.commit(transaction).unwrap();

                self.scene.advance(Duration::from_millis(150)).unwrap();

                let body = &self.scene.bodies[0];

                let center = body.translation + (body.geometry.min + body.geometry.max) * 0.5;

                self.selection = self.scene.raycast(center + Vec3::Y, -Vec3::Y, 2.0).unwrap();

                assert!(matches!(
                    self.selection.map(|hit| hit.target),
                    Some(crate::scene::Target::Dynamic { .. })
                ));

                self.canvas = Canvas::new();

                self.canvas.rect(
                    Rect::new(Vec2::new(2.0, 2.0), Vec2::new(8.0, 8.0)),
                    [0.9, 0.2, 0.1, 0.7],
                );
            }
            4 => {
                self.size = [93, 157];
                self.format = FORMATS[1];
                self.dpi = 1.5;

                self.canvas
                    .set_reference_size(Some(Vec2::new(100.0, 100.0)));

                self.scene.advance(Duration::from_millis(100)).unwrap();

                self.selection = None;
            }
            5 => {
                self.scene = Scene::new(settings()).unwrap();
                self.canvas = Canvas::new();
                self.selection = None;
            }
            6 => {
                self.size = [137, 101];
                self.format = FORMATS[2];
                self.dpi = 1.0;

                place(
                    &mut self.scene,
                    (0..4).flat_map(|x| {
                        (0..4).flat_map(move |y| {
                            (0..4).map(move |z| {
                                (IVec3::new(x, y, z), Voxel::new(MaterialId(2), OwnerId(3)))
                            })
                        })
                    }),
                );

                self.camera = Self::new().camera;

                self.canvas
                    .text("Fresh", Vec2::new(5.0, 5.0), &TextStyle::default());
            }
            _ => unreachable!(),
        }
    }
}

fn shaders(directory: &Path, paths: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();

        if path.is_dir() {
            shaders(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "wgsl")
        {
            paths.push(path);
        }
    }
}

fn assert_same_frame(gpu: &Gpu, stage: usize, fixture: &Fixture, actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len());

    let differences = actual
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .enumerate()
        .filter(|(_, (actual, expected))| actual != expected)
        .collect::<Vec<_>>();

    let first = differences.first().map(|(index, (actual, expected))| {
        (
            index % fixture.size[0] as usize,
            index / fixture.size[0] as usize,
            actual,
            expected,
        )
    });

    assert!(
        differences.is_empty(),
        "stage {stage}: reused renderer differs from a fresh renderer at {} pixels; first (x, y, actual, expected): {first:?}; adapter: {:?}",
        differences.len(),
        gpu.adapter
    );
}

// Checks every shader file in the engine and builds the real drawing pipelines for several
// output formats. Invalid shader code or a mismatch between a shader and its supplied data
// must fail here, before attempting to run the game. Newly added shader files are found automatically.
#[test]
#[ignore = "requires a GPU; run cargo test -p engine render::tests -- --ignored"]
fn shaders_and_render_pipelines_compile() {
    let gpu = Gpu::new();

    let mut paths = Vec::new();

    shaders(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut paths,
    );

    paths.sort();

    assert!(!paths.is_empty());

    for path in paths {
        let source = fs::read_to_string(&path).unwrap();

        gpu.checked(&path.display().to_string(), || {
            let _ = gpu
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: path.to_str(),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
        });
    }

    let mut fixture = Fixture::new();

    for format in FORMATS {
        fixture.format = format;

        gpu.renderer(&fixture);
    }
}

// Runs the real renderer through an empty world, visible blocks, falling pieces, selection
// outlines, and interface changes, then changes the output size and format. Every frame
// must finish on the GPU without errors, and populated scenes must produce more than a
// blank image. This does not require opening a window or matching an approved screenshot.
#[test]
#[ignore = "requires a GPU; run cargo test -p engine render::tests -- --ignored"]
fn rendering_runtime_completes_without_gpu_errors() {
    let gpu = Gpu::new();

    let mut fixture = Fixture::new();

    let mut renderer = gpu.renderer(&fixture);

    let mut output = None;

    for stage in 0..=6 {
        fixture.change(stage);

        for frame in 0..3 {
            let pixels = gpu.render(
                &format!("runtime stage {stage}, frame {frame}"),
                &mut renderer,
                &fixture,
                &mut output,
            );

            assert_eq!(
                pixels.len(),
                fixture.size[0] as usize * fixture.size[1] as usize * 4
            );

            if !matches!(stage, 0 | 5) {
                assert!(
                    pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]),
                    "stage {stage}: populated scene rendered a blank image"
                );
            }
        }
    }
}

// Changes the world, moving objects, interface, selection, screen size, and output format
// while keeping the same renderer alive. After every change, its picture must exactly match
// a brand-new renderer given the current scene. This catches leftovers from earlier frames
// without locking in today's appearance: intentional visual changes affect both pictures.
#[test]
#[ignore = "requires a GPU; run cargo test -p engine render::tests -- --ignored"]
fn reused_renderer_matches_fresh_renderer_after_state_changes() {
    let gpu = Gpu::new();

    let mut fixture = Fixture::new();

    let mut reused = gpu.renderer(&fixture);

    let mut reused_output = None;

    for stage in 0..=6 {
        fixture.change(stage);

        let actual = gpu.render(
            &format!("reused stage {stage}"),
            &mut reused,
            &fixture,
            &mut reused_output,
        );

        let mut fresh = gpu.renderer(&fixture);

        let expected = gpu.render(
            &format!("fresh stage {stage}"),
            &mut fresh,
            &fixture,
            &mut None,
        );

        assert_same_frame(&gpu, stage, &fixture, &actual, &expected);
    }
}
