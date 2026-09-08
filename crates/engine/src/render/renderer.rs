#[cfg(feature = "gpu-bench")]
#[path = "bench.rs"]
pub(super) mod bench;

use super::{overlay::OverlayRenderer, ui::UiRenderer, world::WorldRenderer};
use crate::{
    Aabb,
    graphics::{Frame, Graphics, GraphicsError, Result},
    player::Camera,
    scene::{Hit, Scene},
    ui::Canvas,
};
use glam::Vec3;

pub(super) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    matrix: [[f32; 4]; 4],
    eye_fog: [f32; 4],
    sky_ambient: [f32; 4],
    sun: [f32; 4],
}

#[derive(Clone, Copy)]
pub(super) struct Target<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub size: [u32; 2],
}

pub(crate) struct Renderer {
    format: wgpu::TextureFormat,
    view_layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    binding: wgpu::BindGroup,
    depth: wgpu::TextureView,
    size: [u32; 2],
    clear: wgpu::Color,
    world: WorldRenderer,
    overlay: OverlayRenderer,
    ui: UiRenderer,
}

fn depth(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("world depth"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl Renderer {
    pub(crate) fn new(graphics: &Graphics) -> Result<Self> {
        let presentation = graphics
            .presentation()
            .ok_or(GraphicsError::NotPresenting)?;

        let format = presentation.format;

        let size = [presentation.width, presentation.height];

        graphics.checked(|device, queue| {
            Self::for_target(Target {
                device,
                queue,
                format,
                size,
            })
        })
    }

    pub(super) fn for_target(target: Target<'_>) -> Self {
        let Target {
            device,
            format,
            size,
            ..
        } = target;

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world view"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world view layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });

        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world view binding"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });

        Self {
            format,
            world: WorldRenderer::new(device, format, &view_layout),
            overlay: OverlayRenderer::new(device, format, &view_layout),
            ui: UiRenderer::new(device, format),
            view_layout,
            uniform,
            binding,
            depth: depth(device, size),
            size,
            clear: wgpu::Color::BLACK,
        }
    }

    fn reconfigure(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.world.reconfigure(device, format, &self.view_layout);

        self.overlay.reconfigure(device, format, &self.view_layout);

        self.ui.reconfigure(device, format);

        self.format = format;
    }

    pub(crate) fn prepare(
        &mut self,
        graphics: &Graphics,
        scene: &Scene,
        camera: Camera,
        selection: Option<Hit>,
        canvas: &Canvas,
        dpi: f32,
    ) -> Result<()> {
        let p = graphics
            .presentation()
            .ok_or(GraphicsError::NotPresenting)?;

        if p.format != self.format {
            graphics.checked(|device, _| self.reconfigure(device, p.format))?;
        }

        self.prepare_target(
            Target {
                device: graphics.device(),
                queue: graphics.queue(),
                format: p.format,
                size: [p.width, p.height],
            },
            scene,
            camera,
            selection,
            canvas,
            dpi,
        )?;

        graphics.check_errors()
    }

    pub(super) fn prepare_target(
        &mut self,
        target: Target<'_>,
        scene: &Scene,
        camera: Camera,
        selection: Option<Hit>,
        canvas: &Canvas,
        dpi: f32,
    ) -> Result<()> {
        let Target {
            device,
            queue,
            format,
            size,
        } = target;

        if format != self.format {
            self.reconfigure(device, format);
        }

        if self.size != size {
            self.depth = depth(device, size);
            self.size = size;
        }

        let visuals = &scene.settings.visuals;

        if !camera.position.is_finite()
            || !camera.direction.is_finite()
            || !camera.direction.is_normalized()
            || camera.direction.cross(Vec3::Y).length_squared() < f32::EPSILON
            || !camera.fov_radians.is_finite()
            || !(0.0..std::f32::consts::PI).contains(&camera.fov_radians)
            || camera.fov_radians == 0.0
            || !camera.near_plane.is_finite()
            || camera.near_plane <= 0.0
            || camera.near_plane >= visuals.view_distance
        {
            return Err(GraphicsError::InvalidCamera);
        }

        let matrix = camera.matrix(size[0] as f32 / size[1] as f32, visuals.view_distance);

        let sun = Vec3::from_array(visuals.sunlight).normalize();

        let uniforms = Uniforms {
            matrix: matrix.to_cols_array_2d(),
            eye_fog: camera.position.extend(visuals.fog_distance).to_array(),
            sky_ambient: [
                visuals.sky[0],
                visuals.sky[1],
                visuals.sky[2],
                visuals.ambient,
            ],
            sun: sun.extend(0.0).to_array(),
        };

        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniforms));

        self.clear = wgpu::Color {
            r: visuals.sky[0] as f64,
            g: visuals.sky[1] as f64,
            b: visuals.sky[2] as f64,
            a: 1.0,
        };

        self.world.prepare(
            device,
            queue,
            scene.render_geometry()?,
            matrix,
            camera.position,
        )?;

        let outline = selection
            .and_then(|hit| scene.selection_bounds(hit))
            .map(|bounds| {
                let padding = (bounds.max - bounds.min) * 0.01;

                Aabb {
                    min: bounds.min - padding,
                    max: bounds.max + padding,
                }
            });

        self.overlay.prepare(queue, outline, [1.0, 0.76, 0.25]);

        self.ui.prepare(device, queue, canvas, size, dpi)?;

        Ok(())
    }

    pub(crate) fn draw(&self, frame: &mut Frame) {
        let (view, encoder) = frame.parts();

        self.draw_target(view, encoder);
    }

    pub(super) fn draw_target(&self, view: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            self.world.draw(&mut pass, &self.binding);

            self.overlay.draw(&mut pass, &self.binding);
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });

        self.ui.draw(&mut pass);
    }
}
