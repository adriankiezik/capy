use std::sync::Arc;

use crate::{RenderError, Result};

pub(crate) struct GpuContext {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) timestamp_supported: bool,
    pub(crate) backend: wgpu::Backend,
    #[cfg(feature = "dlss")]
    pub(crate) dlss_extensions_enabled: bool,
    #[cfg(feature = "dlss")]
    pub(crate) dlss_rr_supported: bool,
    #[cfg(feature = "dlss")]
    pub(crate) dlss_fg_supported: bool,
    #[cfg(feature = "dlss")]
    pub(crate) reflex: Option<crate::dlss::reflex::ReflexContext>,
}

impl GpuContext {
    pub(crate) fn new(
        window: Arc<dyn capy_core::Window>,
        width: u32,
        height: u32,
        vsync: bool,
        #[cfg(feature = "dlss")] dlss_project_id: Option<uuid::Uuid>,
    ) -> Result<Self> {
        #[allow(unused_variables)]
        let force_dx12 = std::env::var("CAPY_FORCE_DX12").is_ok();

        // -- Step 1: Try DLSS (Vulkan) unless force-DX12 is set ---------------
        #[cfg(feature = "dlss")]
        let dlss_device = if force_dx12 {
            tracing::info!("CAPY_FORCE_DX12 set — skipping DLSS Vulkan device.");
            None
        } else {
            dlss_project_id
                .map(|project_id| Self::try_create_dlss_device(window.clone(), project_id))
                .transpose()?
                .flatten()
        };

        #[cfg(feature = "dlss")]
        let (
            surface,
            adapter,
            device,
            queue,
            timestamp_supported,
            dlss_extensions_enabled,
            dlss_rr_supported,
            dlss_fg_supported,
            reflex_ctx,
        ) = if let Some((
            surface,
            adapter,
            device,
            queue,
            rr_supported,
            fg_supported,
            reflex_supported,
        )) = dlss_device
        {
            let ts = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
            let reflex_ctx = if reflex_supported {
                crate::dlss::reflex::ReflexContext::new(&device)
            } else {
                None
            };
            (
                surface,
                adapter,
                device,
                queue,
                ts,
                true,
                rr_supported,
                fg_supported,
                reflex_ctx,
            )
        } else {
            let (surface, adapter, device, queue, ts) =
                Self::create_default_device(window.clone())?;
            (
                surface, adapter, device, queue, ts, false, false, false, None,
            )
        };

        #[cfg(not(feature = "dlss"))]
        let (surface, adapter, device, queue, timestamp_supported) =
            Self::create_default_device(window)?;

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

        let backend = adapter.get_info().backend;

        Ok(Self {
            surface,
            adapter,
            device,
            queue,
            config,
            timestamp_supported,
            backend,
            #[cfg(feature = "dlss")]
            dlss_extensions_enabled,
            #[cfg(feature = "dlss")]
            dlss_rr_supported,
            #[cfg(feature = "dlss")]
            dlss_fg_supported,
            #[cfg(feature = "dlss")]
            reflex: reflex_ctx,
        })
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);

        // Swapchain handle changes after reconfigure — re-enable Reflex on the new one.
        #[cfg(feature = "dlss")]
        self.refresh_reflex_swapchain();
    }

    /// Re-enable Reflex on the current swapchain after a surface reconfigure.
    #[cfg(feature = "dlss")]
    pub(crate) fn refresh_reflex_swapchain(&mut self) {
        if let Some(reflex) = &mut self.reflex {
            if reflex.is_enabled() {
                if let Some(sc) = crate::dlss::reflex::raw_swapchain(&self.surface) {
                    // Disable on old (now destroyed) handle is a no-op; enable on new handle.
                    reflex.enable(sc);
                }
            }
        }
    }

    /// Set the desired maximum frame latency. Reconfigures the surface if the
    /// value changed.
    #[cfg(any(feature = "dlss", feature = "fsr"))]
    pub(crate) fn set_frame_latency(&mut self, latency: u32) {
        if self.config.desired_maximum_frame_latency != latency {
            self.config.desired_maximum_frame_latency = latency;
            self.surface.configure(&self.device, &self.config);
            #[cfg(feature = "dlss")]
            self.refresh_reflex_swapchain();
        }
    }

    /// Create a default (non-DLSS) wgpu device.
    ///
    /// When the `fsr` feature is enabled the backend is forced to DX12 so the
    /// FidelityFX SDK can access raw DX12 handles.  Otherwise PRIMARY is used.
    fn create_default_device(
        window: Arc<dyn capy_core::Window>,
    ) -> Result<(
        wgpu::Surface<'static>,
        wgpu::Adapter,
        wgpu::Device,
        wgpu::Queue,
        bool,
    )> {
        #[cfg(feature = "fsr")]
        let backends = {
            tracing::info!("FSR feature enabled — using DX12 backend.");
            wgpu::Backends::DX12
        };
        #[cfg(not(feature = "fsr"))]
        let backends = wgpu::Backends::PRIMARY;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            backend_options: wgpu::BackendOptions {
                dx12: wgpu::Dx12BackendOptions {
                    shader_compiler: wgpu::Dx12Compiler::StaticDxc,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        });

        let surface = instance.create_surface(window)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))?;
        let (device, queue, ts) = Self::request_device(&adapter)?;
        Ok((surface, adapter, device, queue, ts))
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
            bool, // frame_generation_supported
            bool, // reflex_supported
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
                feature_support.frame_generation_supported,
                feature_support.reflex_supported,
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
