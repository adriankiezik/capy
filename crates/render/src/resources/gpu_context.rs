use std::sync::Arc;

use crate::{RenderError, Result};

pub(crate) struct GpuContext {
    pub(crate) surface: wgpu::Surface<'static>,
    #[cfg(any(feature = "dlss", feature = "fsr"))]
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) timestamp_supported: bool,
    #[cfg(feature = "dlss")]
    pub(crate) dlss_extensions_enabled: bool,
    #[cfg(feature = "dlss")]
    pub(crate) dlss_rr_supported: bool,
}

impl GpuContext {
    pub(crate) fn new(
        window: Arc<dyn capy_core::Window>,
        width: u32,
        height: u32,
        vsync: bool,
        #[cfg(feature = "dlss")] dlss_project_id: Option<uuid::Uuid>,
    ) -> Result<Self> {
        #[cfg(feature = "dlss")]
        let (
            surface,
            adapter,
            device,
            queue,
            timestamp_supported,
            dlss_extensions_enabled,
            dlss_rr_supported,
        ) = {
            let dlss_device = dlss_project_id
                .map(|project_id| Self::try_create_dlss_device(window.clone(), project_id))
                .transpose()?
                .flatten();

            if let Some((surface, adapter, device, queue, rr_supported)) = dlss_device {
                let ts = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
                (surface, adapter, device, queue, ts, true, rr_supported)
            } else {
                let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::PRIMARY,
                    ..Default::default()
                });

                let surface = instance.create_surface(window)?;
                let adapter =
                    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                    }))?;
                let (device, queue, ts) = Self::request_device(&adapter)?;
                (surface, adapter, device, queue, ts, false, false)
            }
        };

        #[cfg(not(feature = "dlss"))]
        let (surface, adapter, device, queue, timestamp_supported) = {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::PRIMARY,
                ..Default::default()
            });

            let surface = instance.create_surface(window)?;
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                }))?;
            let (device, queue, timestamp_supported) = Self::request_device(&adapter)?;
            (surface, adapter, device, queue, timestamp_supported)
        };

        #[allow(unused_variables)]
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .or(caps.formats.first())
            .copied()
            .ok_or(RenderError::InvalidAdapter)?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: if vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            #[cfg(any(feature = "dlss", feature = "fsr"))]
            adapter,
            device,
            queue,
            config,
            timestamp_supported,
            #[cfg(feature = "dlss")]
            dlss_extensions_enabled,
            #[cfg(feature = "dlss")]
            dlss_rr_supported,
        })
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue, bool)> {
        let adapter_limits = adapter.limits();
        let mut required_limits = wgpu::Limits::default();
        required_limits.max_storage_buffer_binding_size =
            adapter_limits.max_storage_buffer_binding_size;
        required_limits.max_buffer_size = adapter_limits.max_buffer_size;
        required_limits.max_storage_textures_per_shader_stage =
            adapter_limits.max_storage_textures_per_shader_stage;

        let timestamp_supported = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let required_features = if timestamp_supported {
            wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        };

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("Capy Device"),
                required_features,
                required_limits,
                memory_hints: Default::default(),
                ..Default::default()
            }))?;
        Ok((device, queue, timestamp_supported))
    }

    #[cfg(feature = "dlss")]
    fn try_create_dlss_device(
        window: Arc<dyn capy_core::Window>,
        project_id: uuid::Uuid,
    ) -> Result<
        Option<(
            wgpu::Surface<'static>,
            wgpu::Adapter,
            wgpu::Device,
            wgpu::Queue,
            bool, // ray_reconstruction_supported
        )>,
    > {
        let mut feature_support = crate::dlss::FeatureSupport::default();
        let instance_descriptor = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        };
        let instance = match crate::dlss::create_instance(
            project_id,
            &instance_descriptor,
            &mut feature_support,
        ) {
            Ok(instance) => instance,
            Err(error) => {
                tracing::warn!(
                    "Failed to initialize a DLSS-capable Vulkan instance: {error}. Falling back to the standard GPU path."
                );
                return Ok(None);
            }
        };

        let surface = instance.create_surface(window)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))?;

        if !feature_support.super_resolution_supported {
            tracing::warn!(
                "DLSS Super Resolution is not supported on the selected Vulkan adapter. Falling back to the standard GPU path."
            );
            return Ok(None);
        }

        let adapter_limits = adapter.limits();
        let mut required_limits = wgpu::Limits::default();
        required_limits.max_storage_buffer_binding_size =
            adapter_limits.max_storage_buffer_binding_size;
        required_limits.max_buffer_size = adapter_limits.max_buffer_size;
        required_limits.max_storage_textures_per_shader_stage =
            adapter_limits.max_storage_textures_per_shader_stage;

        // DLSS Ray Reconstruction requires Vulkan bufferDeviceAddress.
        // Requesting EXPERIMENTAL_RAY_QUERY causes wgpu to enable it via
        // VK_KHR_acceleration_structure → VK_KHR_buffer_device_address.
        let mut required_features = if feature_support.ray_reconstruction_supported {
            wgpu::Features::EXPERIMENTAL_RAY_QUERY
        } else {
            wgpu::Features::empty()
        };
        if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            required_features |= wgpu::Features::TIMESTAMP_QUERY;
        }
        // SAFETY: wgpu experimental features are opt-in; we only request
        // ray-query to enable bufferDeviceAddress for DLSS RR.
        let experimental_features = if feature_support.ray_reconstruction_supported {
            unsafe { wgpu::ExperimentalFeatures::enabled() }
        } else {
            wgpu::ExperimentalFeatures::disabled()
        };

        let device_descriptor = wgpu::DeviceDescriptor {
            label: Some("Capy Device"),
            required_features,
            required_limits,
            memory_hints: Default::default(),
            experimental_features,
            ..Default::default()
        };

        match crate::dlss::request_device(
            project_id,
            &adapter,
            &device_descriptor,
            &mut feature_support,
        ) {
            Ok((device, queue)) if feature_support.super_resolution_supported => Ok(Some((
                surface,
                adapter,
                device,
                queue,
                feature_support.ray_reconstruction_supported,
            ))),
            Ok(_) => {
                tracing::warn!(
                    "DLSS Super Resolution is not supported by the created Vulkan device. Falling back to the standard GPU path."
                );
                Ok(None)
            }
            Err(error) => {
                tracing::warn!(
                    "Failed to create a DLSS-capable Vulkan device: {error}. Falling back to the standard GPU path."
                );
                Ok(None)
            }
        }
    }
}
