use super::super::{
    residency::{DrawList, Geometry, Instance, SurfaceVertex},
    visibility::Order,
};
use glam::{Mat4, Vec3};

pub(in crate::render::world) struct SurfaceFrame<'a> {
    pub matrix: Mat4,
    pub eye: Vec3,
    pub view: &'a wgpu::BindGroup,
    pub shadows: &'a wgpu::BindGroup,
    pub shadow_binding_generation: u64,
}

pub(in crate::render::world) struct Surfaces {
    pipeline: wgpu::RenderPipeline,
    draws: DrawList,
    view: Option<(Mat4, Vec3, u64)>,
    visible: Vec<usize>,
    bundle: Option<wgpu::RenderBundle>,
    shadow_binding_generation: u64,
    format: wgpu::TextureFormat,
}

impl Surfaces {
    pub(in crate::render::world) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
        shadow_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        Self {
            pipeline: Self::pipeline(device, format, view_layout, shadow_layout),
            draws: DrawList::new(device),
            view: None,
            visible: Vec::new(),
            bundle: None,
            shadow_binding_generation: 0,
            format,
        }
    }

    fn pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
        shadow_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxel surfaces"),
            source: wgpu::ShaderSource::Wgsl(include_str!("surfaces.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("voxel layout"),
            bind_group_layouts: &[Some(view_layout), Some(shadow_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel greedy surfaces"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<SurfaceVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 2 => Float32x3, 4 => Uint32],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![3 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4],
                    }),
                ],
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
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::render::renderer::DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    pub(in crate::render::world) fn reconfigure(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
        shadow_layout: &wgpu::BindGroupLayout,
    ) {
        self.pipeline = Self::pipeline(device, format, view_layout, shadow_layout);
        self.format = format;
        self.bundle = None;
    }

    pub(in crate::render::world) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geometry: &Geometry,
        frame: SurfaceFrame<'_>,
    ) {
        let SurfaceFrame {
            matrix,
            eye,
            view,
            shadows,
            shadow_binding_generation,
        } = frame;

        let key = (matrix, eye, geometry.visibility_revision());

        if self.view != Some(key) {
            geometry.visible(matrix, Order::Perspective(eye), &mut self.visible);

            self.view = Some(key);
        }

        let commands_changed = self.draws.prepare(device, queue, geometry, &self.visible);

        if commands_changed || self.shadow_binding_generation != shadow_binding_generation {
            self.bundle = None;
        } else if self.bundle.is_none() && !self.visible.is_empty() {
            let mut encoder =
                device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                    label: Some("retained world surfaces"),
                    color_formats: &[Some(self.format)],
                    depth_stencil: Some(wgpu::RenderBundleDepthStencil {
                        format: crate::render::renderer::DEPTH_FORMAT,
                        depth_read_only: false,
                        stencil_read_only: true,
                    }),
                    sample_count: 1,
                    multiview: None,
                });

            encoder.set_pipeline(&self.pipeline);

            encoder.set_bind_group(0, view, &[]);

            encoder.set_bind_group(1, shadows, &[]);

            self.draws.bundle(&mut encoder);

            self.bundle = Some(encoder.finish(&wgpu::RenderBundleDescriptor {
                label: Some("retained world surfaces"),
            }));
        }

        self.shadow_binding_generation = shadow_binding_generation;
    }

    pub(in crate::render::world) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        view: &wgpu::BindGroup,
        shadows: &wgpu::BindGroup,
    ) {
        if let Some(bundle) = &self.bundle {
            pass.execute_bundles(std::iter::once(bundle));

            return;
        }

        pass.set_pipeline(&self.pipeline);

        pass.set_bind_group(0, view, &[]);

        pass.set_bind_group(1, shadows, &[]);

        self.draws.draw(pass);
    }
}
