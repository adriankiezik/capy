use crate::graphics::{GraphicsError, GraphicsSettings, Result};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::sync::Arc;

pub(super) trait WindowTarget:
    HasWindowHandle + HasDisplayHandle + Send + Sync + std::fmt::Debug
{
}

impl<T: HasWindowHandle + HasDisplayHandle + Send + Sync + std::fmt::Debug> WindowTarget for T {}

#[derive(Debug)]
pub(crate) enum FrameOutcome {
    Presented,
    Retry,
}

#[derive(Debug)]
pub(super) struct Presentation {
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) target: Arc<dyn WindowTarget>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Debug)]
pub struct Graphics {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    requested: Option<wgpu::SurfaceConfiguration>,
    pub(super) presentation: Option<Presentation>,
}

impl Graphics {
    pub(crate) async fn new<T>(
        target: Arc<T>,
        width: u32,
        height: u32,
        mut settings: GraphicsSettings,
    ) -> Result<Self>
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + std::fmt::Debug + 'static,
    {
        let target: Arc<dyn WindowTarget> = target;

        settings.instance.display = Some(Box::new(target.clone()));

        let instance = wgpu::Instance::new(settings.instance);

        let surface = instance.create_surface(target.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..settings.adapter
            })
            .await?;

        let supported = adapter.features();

        let missing = settings.device.required_features - supported;

        if !missing.is_empty() {
            return Err(GraphicsError::MissingFeatures(missing));
        }

        settings.device.required_features |= settings.preferred_features & supported;

        let (device, queue) = adapter.request_device(&settings.device).await?;

        let mut graphics = Self {
            instance,
            adapter,
            device,
            queue,
            requested: settings.surface,
            presentation: None,
        };

        graphics.configure(surface, target, width, height)?;

        Ok(graphics)
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn presentation(&self) -> Option<&wgpu::SurfaceConfiguration> {
        self.presentation.as_ref().map(|p| &p.config)
    }

    pub fn surface_capabilities(&self) -> Result<wgpu::SurfaceCapabilities> {
        Ok(self
            .presentation
            .as_ref()
            .ok_or(GraphicsError::NotPresenting)?
            .surface
            .get_capabilities(&self.adapter))
    }

    pub fn checked<T>(
        &self,
        operation: impl FnOnce(&wgpu::Device, &wgpu::Queue) -> T,
    ) -> Result<T> {
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);

        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);

        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);

        let result = operation(&self.device, &self.queue);

        let internal_error = pollster::block_on(internal.pop());

        let memory_error = pollster::block_on(memory.pop());

        let validation_error = pollster::block_on(validation.pop());

        if let Some(error) = internal_error.or(memory_error).or(validation_error) {
            return Err(error.into());
        }

        Ok(result)
    }

    fn select(
        &self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        requested: Option<&wgpu::SurfaceConfiguration>,
    ) -> Result<wgpu::SurfaceConfiguration> {
        let maximum = self.device.limits().max_texture_dimension_2d;

        if width > maximum || height > maximum {
            return Err(GraphicsError::SurfaceSize {
                width,
                height,
                maximum,
            });
        }

        let caps = surface.get_capabilities(&self.adapter);

        let mut config = match requested {
            Some(config) => config.clone(),
            None => surface
                .get_default_config(&self.adapter, width.max(1), height.max(1))
                .ok_or(GraphicsError::UnsupportedSurface)?,
        };

        config.width = width.max(1);
        config.height = height.max(1);

        if !caps.formats.contains(&config.format) {
            return Err(GraphicsError::SurfaceFormat(config.format));
        }

        if !matches!(
            config.present_mode,
            wgpu::PresentMode::AutoVsync | wgpu::PresentMode::AutoNoVsync
        ) && !caps.present_modes.contains(&config.present_mode)
        {
            return Err(GraphicsError::PresentMode(config.present_mode));
        }

        if config.alpha_mode != wgpu::CompositeAlphaMode::Auto
            && !caps.alpha_modes.contains(&config.alpha_mode)
        {
            return Err(GraphicsError::AlphaMode(config.alpha_mode));
        }

        if !caps.usages.contains(config.usage) {
            return Err(GraphicsError::SurfaceUsage(config.usage));
        }

        if config.desired_maximum_frame_latency == 0 {
            return Err(GraphicsError::ZeroFrameLatency);
        }

        Ok(config)
    }

    fn configure(
        &mut self,
        surface: wgpu::Surface<'static>,
        target: Arc<dyn WindowTarget>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let config = self.select(&surface, width, height, self.requested.as_ref())?;

        if width > 0 && height > 0 {
            self.checked(|device, _| surface.configure(device, &config))?;
        }

        self.presentation = Some(Presentation {
            surface,
            target,
            config,
            width,
            height,
        });

        Ok(())
    }

    pub fn configure_presentation(
        &mut self,
        configuration: wgpu::SurfaceConfiguration,
    ) -> Result<()> {
        let p = self
            .presentation
            .as_ref()
            .ok_or(GraphicsError::NotPresenting)?;

        let config = self.select(&p.surface, p.width, p.height, Some(&configuration))?;

        if p.width > 0 && p.height > 0 {
            self.checked(|device, _| p.surface.configure(device, &config))?;
        }

        self.presentation
            .as_mut()
            .ok_or(GraphicsError::NotPresenting)?
            .config = config;
        self.requested = Some(configuration);

        Ok(())
    }

    pub(crate) fn attach<T>(&mut self, target: Arc<T>, width: u32, height: u32) -> Result<()>
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + std::fmt::Debug + 'static,
    {
        self.attach_target(target, width, height)
    }

    pub(super) fn attach_target(
        &mut self,
        target: Arc<dyn WindowTarget>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.detach();

        let surface = self.instance.create_surface(target.clone())?;

        self.configure(surface, target, width, height)
    }

    pub(crate) fn detach(&mut self) {
        self.presentation = None;
    }

    pub(crate) fn drawable(&self) -> bool {
        self.presentation
            .as_ref()
            .is_some_and(|p| p.width > 0 && p.height > 0)
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        let Some(p) = &self.presentation else {
            return Ok(());
        };

        if (p.width, p.height) == (width, height) {
            return Ok(());
        }

        let config = self.select(&p.surface, width, height, Some(&p.config))?;

        if width > 0 && height > 0 {
            self.checked(|device, _| p.surface.configure(device, &config))?;
        }

        let p = self
            .presentation
            .as_mut()
            .ok_or(GraphicsError::NotPresenting)?;

        p.width = width;
        p.height = height;
        p.config = config;

        Ok(())
    }
}
