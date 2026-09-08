use super::super::{
    residency::{Geometry, Instance},
    visibility::Order,
};
use crate::scene::Vertex;
use glam::{Mat4, Vec3};

pub(in crate::render::world) struct Surfaces {
    pipeline: wgpu::RenderPipeline,
    visible: Vec<usize>,
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
            visible: Vec::new(),
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
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 4 => Float32],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![3 => Float32x4, 5 => Float32x4],
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
    }

    pub(in crate::render::world) fn prepare(
        &mut self,
        geometry: &Geometry,
        matrix: Mat4,
        eye: Vec3,
    ) {
        geometry.visible(matrix, Order::Perspective(eye), &mut self.visible);
    }

    pub(in crate::render::world) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        geometry: &Geometry,
        view: &wgpu::BindGroup,
        shadows: &wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.pipeline);

        pass.set_bind_group(0, view, &[]);

        pass.set_bind_group(1, shadows, &[]);

        geometry.draw(pass, &self.visible);
    }
}
