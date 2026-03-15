pub(crate) struct EguiOverlayRenderer {
    pub(crate) renderer: egui_wgpu::Renderer,
    pub(crate) surface_format: wgpu::TextureFormat,
}

impl EguiOverlayRenderer {
    pub(crate) fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let renderer = egui_wgpu::Renderer::new(
            device,
            surface_format,
            egui_wgpu::RendererOptions::default(),
        );
        Self {
            renderer,
            surface_format,
        }
    }
}
