use super::{
    residency::Geometry,
    shadows::{ShadowFrame, Shadows},
    surfaces::{SurfaceFrame, Surfaces},
};
use crate::{
    graphics::Result,
    replica::{MeshInstance, ShadowSettings},
};
use glam::{Mat4, Vec3};

pub(in crate::render) struct WorldView {
    pub(in crate::render) matrix: Mat4,
    pub(in crate::render) eye: Vec3,
    pub(in crate::render) sun: Vec3,
    pub(in crate::render) near_plane: f32,
    pub(in crate::render) shadows: Option<ShadowSettings>,
}

pub(in crate::render) struct WorldRenderer {
    geometry: Geometry,
    surfaces: Surfaces,
    shadows: Shadows,
}

impl WorldRenderer {
    pub(in crate::render) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shadows = Shadows::new(device);

        Self {
            surfaces: Surfaces::new(device, format, view_layout, shadows.layout()),
            shadows,
            geometry: Geometry::new(),
        }
    }

    pub(in crate::render) fn reconfigure(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
    ) {
        self.surfaces
            .reconfigure(device, format, view_layout, self.shadows.layout());
    }

    pub(in crate::render) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sources: std::sync::Arc<[MeshInstance]>,
        view: WorldView,
        binding: &wgpu::BindGroup,
    ) -> Result<()> {
        let WorldView {
            matrix,
            eye,
            sun,
            near_plane,
            shadows,
        } = view;

        let geometry_changed = self.geometry.prepare(device, sources)?;

        self.shadows.prepare(
            device,
            queue,
            ShadowFrame {
                matrix,
                eye,
                sun,
                near_plane,
                settings: shadows,
                geometry_changed,
            },
            &self.geometry,
        )?;

        self.surfaces.prepare(
            device,
            queue,
            &self.geometry,
            SurfaceFrame {
                matrix,
                eye,
                view: binding,
                shadows: self.shadows.binding(),
                shadow_binding_generation: self.shadows.binding_generation(),
            },
        );

        Ok(())
    }

    #[cfg(feature = "render-bench")]
    pub(in crate::render) fn geometry_bytes(&self) -> usize {
        self.geometry.bytes()
    }

    pub(in crate::render) fn draw_shadows(&self, encoder: &mut wgpu::CommandEncoder) {
        self.shadows.draw(encoder);
    }

    pub(in crate::render) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, view: &wgpu::BindGroup) {
        self.surfaces.draw(pass, view, self.shadows.binding());
    }
}
