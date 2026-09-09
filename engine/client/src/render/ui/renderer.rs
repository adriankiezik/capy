use crate::{
    graphics::{GraphicsError, Result},
    ui::{Canvas, GlyphCache, Vertex},
};
use glam::Vec2;

pub(in crate::render) struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    binding: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    count: u32,
    capacity: usize,
    glyphs: GlyphCache,
}

impl UiRenderer {
    pub(in crate::render) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui screen layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });

        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui screen binding"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });

        Self {
            pipeline: Self::pipeline(device, format, &layout),
            layout,
            uniform,
            binding,
            vertices: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("canvas geometry"),
                size: 4,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            count: 0,
            capacity: 0,
            glyphs: GlyphCache::default(),
        }
    }

    fn pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        screen_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui layout"),
            bind_group_layouts: &[Some(screen_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

    pub(in crate::render) fn reconfigure(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) {
        self.pipeline = Self::pipeline(device, format, &self.layout);
    }

    pub(in crate::render) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &Canvas,
        size: [u32; 2],
        dpi: f32,
    ) -> Result<()> {
        let vertices = canvas
            .vertices(
                Vec2::new(size[0] as f32, size[1] as f32),
                dpi,
                &mut self.glyphs,
            )
            .map_err(|error| GraphicsError::Draw(error.into()))?;

        let count = u32::try_from(vertices.len()).map_err(|_| GraphicsError::ResourceLimit)?;

        if vertices.len() > self.capacity {
            let capacity = vertices
                .len()
                .checked_next_power_of_two()
                .ok_or(GraphicsError::ResourceLimit)?;

            if capacity as u64
                > device.limits().max_buffer_size / std::mem::size_of::<Vertex>() as u64
            {
                return Err(GraphicsError::ResourceLimit);
            }

            self.vertices = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("canvas geometry"),
                size: capacity as u64 * std::mem::size_of::<Vertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.capacity = capacity;
        }

        let screen = [size[0] as f32, size[1] as f32, 0.0, 0.0];

        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&screen));

        if !vertices.is_empty() {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices));
        }

        self.count = count;

        Ok(())
    }

    pub(in crate::render) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);

        pass.set_bind_group(0, &self.binding, &[]);

        pass.set_vertex_buffer(0, self.vertices.slice(..));

        pass.draw(0..self.count, 0..1);
    }
}
