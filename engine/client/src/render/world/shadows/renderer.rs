use super::super::{
    residency::{Geometry, Instance},
    visibility::Order,
};
use super::cascades::{COUNT, Cascades};
use crate::{
    graphics::{GraphicsError, Result},
    replica::{ShadowSettings, Vertex},
};
use bytemuck::Zeroable;
use glam::{Mat4, Vec3};
use std::cell::Cell;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    matrices: [[[f32; 4]; 4]; COUNT],
    splits: [f32; 4],
    direction: [f32; 4],
    parameters: [f32; 4],
}

pub(in crate::render::world) struct ShadowFrame {
    pub(in crate::render::world) matrix: Mat4,
    pub(in crate::render::world) eye: Vec3,
    pub(in crate::render::world) sun: Vec3,
    pub(in crate::render::world) near_plane: f32,
    pub(in crate::render::world) settings: Option<ShadowSettings>,
    pub(in crate::render::world) geometry_changed: bool,
}

pub(in crate::render::world) struct Shadows {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    binding: wgpu::BindGroup,
    views: [wgpu::TextureView; COUNT],
    lights: [wgpu::BindGroup; COUNT],
    matrices: [Mat4; COUNT],
    dirty: [Cell<bool>; COUNT],
    casters: [Vec<usize>; COUNT],
    uniform: wgpu::Buffer,
    light_uniforms: [wgpu::Buffer; COUNT],
    sampler: wgpu::Sampler,
    resolution: u32,
    enabled: bool,
}

impl Shadows {
    pub(in crate::render::world) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow sampling layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Uniforms>() as u64
                        ),
                    },
                    count: None,
                },
            ],
        });

        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow light layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("surface shadow"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow pipeline layout"),
            bind_group_layouts: &[Some(&light_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("surface shadow"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![3 => Float32x4],
                    }),
                ],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow sampling uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let light_uniforms = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shadow light matrix"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        let lights = std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadow light"),
                layout: &light_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: light_uniforms[i].as_entire_binding(),
                }],
            })
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow comparison"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let (views, binding) = targets(device, &layout, &sampler, &uniform, 1);

        Self {
            pipeline,
            layout,
            binding,
            views,
            lights,
            matrices: [Mat4::IDENTITY; COUNT],
            dirty: std::array::from_fn(|_| Cell::new(false)),
            casters: std::array::from_fn(|_| Vec::new()),
            uniform,
            light_uniforms,
            sampler,
            resolution: 1,
            enabled: false,
        }
    }

    pub(in crate::render::world) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: ShadowFrame,
        geometry: &Geometry,
    ) -> Result<()> {
        let ShadowFrame {
            matrix,
            eye,
            sun,
            near_plane,
            settings,
            geometry_changed,
        } = frame;

        let settings = settings.filter(|s| s.distance > near_plane);

        let resolution = settings.map_or(1, |s| s.resolution);

        if resolution > device.limits().max_texture_dimension_2d {
            return Err(GraphicsError::ResourceLimit);
        }

        let resized = resolution != self.resolution;

        if resized {
            (self.views, self.binding) = targets(
                device,
                &self.layout,
                &self.sampler,
                &self.uniform,
                resolution,
            );
            self.resolution = resolution;
        }

        let mut uniforms = Uniforms::zeroed();

        if let Some(settings) = settings {
            let cascades = Cascades::new(matrix, eye, sun, geometry.bounds(), settings);

            for i in 0..COUNT {
                self.dirty[i].set(
                    self.dirty[i].get()
                        || resized
                        || !self.enabled
                        || geometry_changed
                        || self.matrices[i] != cascades.matrices[i],
                );

                if self.dirty[i].get() {
                    geometry.visible(
                        cascades.matrices[i],
                        Order::Directional(sun),
                        &mut self.casters[i],
                    );

                    queue.write_buffer(
                        &self.light_uniforms[i],
                        0,
                        bytemuck::cast_slice(&cascades.matrices[i].to_cols_array()),
                    );
                }
            }

            self.matrices = cascades.matrices;
            uniforms.matrices = cascades.matrices.map(|m| m.to_cols_array_2d());
            uniforms.splits = cascades.splits;
            uniforms.direction = cascades.direction.extend(0.0).to_array();
            uniforms.parameters = [1.0, 1.0 / resolution as f32, 0.0, 0.0];
        }

        if settings.is_none() {
            for dirty in &self.dirty {
                dirty.set(false);
            }
        }

        self.enabled = settings.is_some();

        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniforms));

        Ok(())
    }

    pub(in crate::render::world) fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub(in crate::render::world) fn binding(&self) -> &wgpu::BindGroup {
        &self.binding
    }

    pub(in crate::render::world) fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        geometry: &Geometry,
    ) {
        for i in 0..COUNT {
            if !self.dirty[i].get() {
                continue;
            }

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world shadow cascade"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.views[i],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);

            pass.set_bind_group(0, &self.lights[i], &[]);

            geometry.draw(&mut pass, &self.casters[i]);

            self.dirty[i].set(false);
        }
    }
}

fn targets(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform: &wgpu::Buffer,
    resolution: u32,
) -> ([wgpu::TextureView; COUNT], wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cascaded shadow depth"),
        size: wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: COUNT as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let array = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });

    let views = std::array::from_fn(|i| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: i as u32,
            array_layer_count: Some(1),
            ..Default::default()
        })
    });

    let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow sampling"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&array),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            },
        ],
    });

    (views, binding)
}
