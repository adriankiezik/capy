use engine::anyhow::{Context as _, Result};
use engine::{Application, Context, graphics::Frame, runtime::Update, wgpu, winit};
use wgpu::util::DeviceExt;

#[derive(Debug)]
pub struct Game {
    vertices: wgpu::Buffer,
    shader: wgpu::ShaderModule,
    pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
}

impl Game {
    pub fn new(context: &mut Context<'_>) -> Result<Self> {
        let graphics = context.graphics();

        let format = graphics
            .presentation()
            .context("Presentation is unavailable")?
            .format;

        let data: Vec<u8> = [
            [0.0f32, 0.65, 1.0, 0.25, 0.3],
            [-0.65, -0.55, 0.2, 0.85, 0.65],
            [0.65, -0.55, 0.25, 0.5, 1.0],
        ]
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect();

        graphics
            .checked(|device, _| {
                let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("triangle vertices"),
                    contents: &data,
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("triangle"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("shaders/triangle.wgsl").into()),
                });

                let pipeline = Self::pipeline(device, &shader, format);

                Self {
                    vertices,
                    shader,
                    pipeline,
                    format,
                }
            })
            .context("Creating triangle resources")
    }

    fn pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("triangle"),
            layout: None,
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    }
}

impl Application for Game {
    fn update(&mut self, mut update: Update<'_>) -> Result<()> {
        if update.input().pressed(winit::keyboard::KeyCode::F11) {
            let window = update.context().window().native();

            window.set_fullscreen(if window.fullscreen().is_none() {
                Some(winit::window::Fullscreen::Borderless(
                    window.current_monitor(),
                ))
            } else {
                None
            });
        }

        if update.input().pressed(winit::keyboard::KeyCode::Space) {
            let graphics = update.context().graphics();

            let mut settings = graphics
                .presentation()
                .context("Presentation is unavailable")?
                .clone();

            settings.present_mode = if settings.present_mode == wgpu::PresentMode::AutoNoVsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            };

            graphics.configure_presentation(settings)?;
        }

        let graphics = update.context().graphics();

        let format = graphics
            .presentation()
            .context("Presentation is unavailable")?
            .format;

        if format != self.format {
            self.pipeline =
                graphics.checked(|device, _| Self::pipeline(device, &self.shader, format))?;
            self.format = format;
        }

        Ok(())
    }

    fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        let (view, encoder) = frame.parts();

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.015,
                        g: 0.025,
                        b: 0.045,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });

        pass.set_pipeline(&self.pipeline);

        pass.set_vertex_buffer(0, self.vertices.slice(..));

        pass.draw(0..3, 0..1);

        Ok(())
    }
}
