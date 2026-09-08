use crate::Aabb;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

pub(in crate::render) struct OverlayRenderer {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    count: u32,
}

impl OverlayRenderer {
    pub(in crate::render) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        Self {
            pipeline: Self::pipeline(device, format, view_layout),
            vertices: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("overlay lines"),
                size: (24 * std::mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            count: 0,
        }
    }

    fn pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay lines"),
            source: wgpu::ShaderSource::Wgsl(include_str!("lines.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay layout"),
            bind_group_layouts: &[Some(view_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay lines"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::render::renderer::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    pub(in crate::render) fn reconfigure(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
    ) {
        self.pipeline = Self::pipeline(device, format, view_layout);
    }

    pub(in crate::render) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        outline: Option<Aabb>,
        color: [f32; 3],
    ) {
        self.count = 0;

        let Some(bounds) = outline else {
            return;
        };

        let mut vertices = Vec::with_capacity(24);

        for axis in 0..3 {
            for a in 0..2 {
                for b in 0..2 {
                    let mut p = bounds.min;

                    p[(axis + 1) % 3] = if a == 0 {
                        bounds.min[(axis + 1) % 3]
                    } else {
                        bounds.max[(axis + 1) % 3]
                    };
                    p[(axis + 2) % 3] = if b == 0 {
                        bounds.min[(axis + 2) % 3]
                    } else {
                        bounds.max[(axis + 2) % 3]
                    };

                    for end in [bounds.min[axis], bounds.max[axis]] {
                        p[axis] = end;

                        vertices.push(Vertex {
                            position: p.to_array(),
                            color,
                        });
                    }
                }
            }
        }

        self.count = vertices.len() as u32;

        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices));
    }

    pub(in crate::render) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, view: &wgpu::BindGroup) {
        if self.count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);

        pass.set_bind_group(0, view, &[]);

        pass.set_vertex_buffer(0, self.vertices.slice(..));

        pass.draw(0..self.count, 0..1);
    }
}
