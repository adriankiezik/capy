use crate::{
    graphics::{Frame, Graphics, GraphicsError, Result},
    player::Camera,
    scene::{Hit, Mesh, Scene, Target, Vertex},
    ui::{Canvas, GlyphCache},
    world::VOXEL_SIZE,
};
use glam::{Mat4, Vec3, Vec4};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    matrix: [[f32; 4]; 4],
    eye_fog: [f32; 4],
    sky_ambient: [f32; 4],
    sun: [f32; 4],
    screen: [f32; 4],
}

type Key = (u64, [i32; 3]);

struct Resident {
    source: Arc<Mesh>,
    buffer: wgpu::Buffer,
}

pub(crate) struct Renderer {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    hud_pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    binding: wgpu::BindGroup,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    depth: wgpu::TextureView,
    size: [u32; 2],
    meshes: BTreeMap<Key, Resident>,
    visible: Vec<Key>,
    selection: wgpu::Buffer,
    selection_count: u32,
    hud: wgpu::Buffer,
    hud_count: u32,
    hud_capacity: usize,
    glyphs: GlyphCache,
    clear: wgpu::Color,
}

fn vertices() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];

    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

fn depth(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("voxel depth"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
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

        graphics.checked(|device, _| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("voxel surfaces"),
                source: wgpu::ShaderSource::Wgsl(include_str!("voxel.wgsl").into()),
            });

            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("camera"),
                size: std::mem::size_of::<Uniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

            let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("camera binding"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("voxel layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });

            let pipeline = |hud: bool, line: bool| {
                let mut buffers = vec![Some(vertices())];

                if !hud {
                    buffers.push(Some(wgpu::VertexBufferLayout {
                        array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![3 => Float32x4],
                    }));
                }

                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(if hud {
                        "hud"
                    } else if line {
                        "selection"
                    } else {
                        "voxel greedy surfaces"
                    }),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(if hud { "vs_hud" } else { "vs_main" }),
                        compilation_options: Default::default(),
                        buffers: &buffers,
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(if hud { "fs_hud" } else { "fs_main" }),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: hud.then_some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: if line {
                            wgpu::PrimitiveTopology::LineList
                        } else {
                            wgpu::PrimitiveTopology::TriangleList
                        },
                        cull_mode: if hud || line {
                            None
                        } else {
                            Some(wgpu::Face::Back)
                        },
                        ..Default::default()
                    },
                    depth_stencil: if hud {
                        None
                    } else {
                        Some(wgpu::DepthStencilState {
                            format: wgpu::TextureFormat::Depth32Float,
                            depth_write_enabled: Some(!line),
                            depth_compare: Some(wgpu::CompareFunction::LessEqual),
                            stencil: Default::default(),
                            bias: Default::default(),
                        })
                    },
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            };

            let instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rigid instances"),
                size: 16,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let selection = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("selection lines"),
                size: (24 * std::mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let hud = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hud"),
                size: 4,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            Self {
                format,
                pipeline: pipeline(false, false),
                lines: pipeline(false, true),
                hud_pipeline: pipeline(true, false),
                uniform,
                binding,
                instances,
                instance_capacity: 1,
                depth: depth(device, size),
                size,
                meshes: BTreeMap::new(),
                visible: Vec::new(),
                selection,
                selection_count: 0,
                hud,
                hud_count: 0,
                hud_capacity: 0,
                glyphs: GlyphCache::default(),
                clear: wgpu::Color::BLACK,
            }
        })
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
            *self = Self::new(graphics)?;
        }

        let device = graphics.device();

        let queue = graphics.queue();

        let size = [p.width, p.height];

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
            screen: [size[0] as f32, size[1] as f32, 0.0, 0.0],
        };

        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniforms));

        self.clear = wgpu::Color {
            r: visuals.sky[0] as f64,
            g: visuals.sky[1] as f64,
            b: visuals.sky[2] as f64,
            a: 1.0,
        };

        let mut sources = Vec::new();

        let mut available = scene.settings.max_mesh_vertices;

        for (&key, source) in &scene.meshes {
            let mesh = source.resolve(
                key,
                scene.settings.render_leaf_edge,
                |p| scene.world.resident_voxel(p),
                &scene.world,
                available,
            )?;

            available -= mesh.vertices.len();

            sources.push(((0, key), mesh, Vec3::ZERO));
        }

        for body in &scene.bodies {
            for (&key, source) in &body.geometry.meshes {
                let mesh = source.resolve(
                    key,
                    scene.settings.render_leaf_edge,
                    |p| body.geometry.voxel(p),
                    &scene.world,
                    available,
                )?;

                available -= mesh.vertices.len();

                sources.push(((body.id, key), mesh, body.translation));
            }
        }

        let mut retained = BTreeSet::new();

        let mut draws = Vec::new();

        for (key, mesh, translation) in sources {
            if mesh.vertices.is_empty() {
                continue;
            }

            if std::mem::size_of_val(mesh.vertices.as_slice()) as u64
                > device.limits().max_buffer_size
                || mesh.vertices.len() > u32::MAX as usize
            {
                return Err(GraphicsError::ResourceLimit);
            }

            retained.insert(key);

            if !self
                .meshes
                .get(&key)
                .is_some_and(|resident| Arc::ptr_eq(&resident.source, &mesh))
            {
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("immutable voxel leaf"),
                    contents: bytemuck::cast_slice(&mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                self.meshes.insert(
                    key,
                    Resident {
                        source: mesh.clone(),
                        buffer,
                    },
                );
            }

            let origin = mesh.origin + translation;

            let min = mesh.min + origin;

            let max = mesh.max + origin;

            if visible(matrix, min, max) {
                draws.push((
                    camera.position.distance_squared((min + max) * 0.5),
                    key,
                    origin.extend(0.0).to_array(),
                ));
            }
        }

        self.meshes.retain(|key, _| retained.contains(key));

        draws.sort_by(|a, b| a.0.total_cmp(&b.0));

        self.visible = draws.iter().map(|d| d.1).collect();

        let mut instances: Vec<[f32; 4]> = draws.into_iter().map(|d| d.2).collect();

        instances.push([0.0; 4]);

        if instances.len() > self.instance_capacity {
            let capacity = instances
                .len()
                .checked_next_power_of_two()
                .ok_or(GraphicsError::ResourceLimit)?;

            if capacity as u64 > device.limits().max_buffer_size / 16
                || instances.len() > u32::MAX as usize
            {
                return Err(GraphicsError::ResourceLimit);
            }

            self.instance_capacity = capacity;
            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rigid instances"),
                size: (self.instance_capacity * 16) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));

        self.selection_count = 0;

        if let Some(hit) = selection {
            let position = match hit.target {
                Target::Static(voxel) => Some(voxel.as_vec3() * VOXEL_SIZE),
                Target::Dynamic { body, voxel } => scene
                    .bodies
                    .iter()
                    .find(|b| b.id == body)
                    .map(|b| voxel.as_vec3() * VOXEL_SIZE + b.translation),
            };

            if let Some(position) = position {
                let mut vertices = Vec::new();

                let min = position - Vec3::splat(VOXEL_SIZE * 0.01);

                let max = position + Vec3::splat(VOXEL_SIZE * 1.01);

                for axis in 0..3 {
                    for a in 0..2 {
                        for b in 0..2 {
                            let mut p = min;

                            p[(axis + 1) % 3] = if a == 0 {
                                min[(axis + 1) % 3]
                            } else {
                                max[(axis + 1) % 3]
                            };
                            p[(axis + 2) % 3] = if b == 0 {
                                min[(axis + 2) % 3]
                            } else {
                                max[(axis + 2) % 3]
                            };

                            for end in [min[axis], max[axis]] {
                                p[axis] = end;

                                vertices.push(Vertex {
                                    position: p.to_array(),
                                    normal: [0.0; 3],
                                    color: [1.0, 0.76, 0.25],
                                });
                            }
                        }
                    }
                }

                self.selection_count = vertices.len() as u32;

                queue.write_buffer(&self.selection, 0, bytemuck::cast_slice(&vertices));
            }
        }

        let vertices = canvas
            .vertices(
                glam::Vec2::new(size[0] as f32, size[1] as f32),
                dpi,
                &mut self.glyphs,
            )
            .map_err(|error| GraphicsError::Draw(error.into()))?;

        self.hud_count = vertices.len() as u32;

        if vertices.len() > self.hud_capacity {
            let capacity = vertices
                .len()
                .checked_next_power_of_two()
                .ok_or(GraphicsError::ResourceLimit)?;

            let bytes = capacity as u64 * std::mem::size_of::<Vertex>() as u64;

            if bytes > device.limits().max_buffer_size {
                return Err(GraphicsError::ResourceLimit);
            }

            self.hud_capacity = capacity;
            self.hud = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("canvas geometry"),
                size: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        if !vertices.is_empty() {
            queue.write_buffer(&self.hud, 0, bytemuck::cast_slice(&vertices));
        }

        graphics.check_errors()
    }

    pub(crate) fn draw(&self, frame: &mut Frame) {
        let (view, encoder) = frame.parts();

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("voxel scene"),
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

            pass.set_pipeline(&self.pipeline);

            pass.set_bind_group(0, &self.binding, &[]);

            pass.set_vertex_buffer(1, self.instances.slice(..));

            for (index, key) in self.visible.iter().enumerate() {
                if let Some(mesh) = self.meshes.get(key) {
                    pass.set_vertex_buffer(0, mesh.buffer.slice(..));

                    pass.draw(
                        0..mesh.source.vertices.len() as u32,
                        index as u32..index as u32 + 1,
                    );
                }
            }

            if self.selection_count > 0 {
                pass.set_pipeline(&self.lines);

                pass.set_vertex_buffer(0, self.selection.slice(..));

                pass.draw(
                    0..self.selection_count,
                    self.visible.len() as u32..self.visible.len() as u32 + 1,
                );
            }
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("overlay"),
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

        pass.set_pipeline(&self.hud_pipeline);

        pass.set_bind_group(0, &self.binding, &[]);

        pass.set_vertex_buffer(0, self.hud.slice(..));

        pass.draw(0..self.hud_count, 0..1);
    }
}

fn visible(matrix: Mat4, min: Vec3, max: Vec3) -> bool {
    let corners: [Vec4; 8] = std::array::from_fn(|i| {
        matrix
            * Vec4::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
                1.0,
            )
    });

    !(0..6).any(|plane| {
        corners.iter().all(|p| match plane {
            0 => p.x < -p.w,
            1 => p.x > p.w,
            2 => p.y < -p.w,
            3 => p.y > p.w,
            4 => p.z < 0.0,
            _ => p.z > p.w,
        })
    })
}
