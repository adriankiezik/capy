#[derive(Debug)]
pub struct GraphicsSettings {
    pub(super) instance: wgpu::InstanceDescriptor,
    pub(super) adapter: wgpu::RequestAdapterOptions<'static, 'static>,
    pub(super) device: wgpu::DeviceDescriptor<'static>,
    pub(super) preferred_features: wgpu::Features,
    pub(super) surface: Option<wgpu::SurfaceConfiguration>,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            instance: wgpu::InstanceDescriptor::new_without_display_handle(),
            adapter: wgpu::RequestAdapterOptions::default(),
            device: wgpu::DeviceDescriptor::default(),
            preferred_features: wgpu::Features::empty(),
            surface: None,
        }
    }
}

impl GraphicsSettings {
    pub fn instance_mut(&mut self) -> &mut wgpu::InstanceDescriptor {
        &mut self.instance
    }

    pub fn adapter_mut(&mut self) -> &mut wgpu::RequestAdapterOptions<'static, 'static> {
        &mut self.adapter
    }

    pub fn device_mut(&mut self) -> &mut wgpu::DeviceDescriptor<'static> {
        &mut self.device
    }

    #[must_use]
    pub fn with_preferred_features(mut self, features: wgpu::Features) -> Self {
        self.preferred_features = features;

        self
    }

    #[must_use]
    pub fn with_surface(mut self, configuration: wgpu::SurfaceConfiguration) -> Self {
        self.surface = Some(configuration);

        self
    }
}
