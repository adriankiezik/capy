use ash::vk;
use wgpu::{Device, hal::api::Vulkan};

/// Wraps the `VK_NV_low_latency2` Vulkan extension for NVIDIA Reflex support.
///
/// Provides CPU frame pacing (`sleep`) and latency markers that allow the driver
/// to minimise the render queue and reduce end-to-end latency. Reflex is also a
/// mandatory prerequisite for DLSS Frame Generation.
pub(crate) struct ReflexContext {
    low_latency: ash::nv::low_latency2::Device,
    sleep_semaphore: vk::Semaphore,
    raw_device: ash::Device,
    frame_id: u64,
    enabled: bool,
}

impl ReflexContext {
    /// Creates a new Reflex context if `VK_NV_low_latency2` is available.
    ///
    /// Must be called after the Vulkan device has been created with the extension enabled.
    pub(crate) fn new(device: &Device) -> Option<Self> {
        unsafe {
            let hal_device = device.as_hal::<Vulkan>()?;
            let shared_instance = hal_device.shared_instance();
            let raw_instance = shared_instance.raw_instance();
            let raw_device_handle = hal_device.raw_device();

            let low_latency = ash::nv::low_latency2::Device::new(raw_instance, raw_device_handle);

            // Create a timeline semaphore for latency_sleep.
            let mut semaphore_type_info = vk::SemaphoreTypeCreateInfo::default()
                .semaphore_type(vk::SemaphoreType::TIMELINE)
                .initial_value(0);
            let semaphore_info =
                vk::SemaphoreCreateInfo::default().push_next(&mut semaphore_type_info);
            let sleep_semaphore = raw_device_handle
                .create_semaphore(&semaphore_info, None)
                .ok()?;

            Some(Self {
                low_latency,
                sleep_semaphore,
                raw_device: raw_device_handle.clone(),
                frame_id: 0,
                enabled: false,
            })
        }
    }

    /// Enable low-latency mode on the given swapchain.
    pub(crate) fn enable(&mut self, swapchain: vk::SwapchainKHR) {
        if self.enabled {
            return;
        }
        let info = vk::LatencySleepModeInfoNV::default()
            .low_latency_mode(true)
            .low_latency_boost(true)
            .minimum_interval_us(0);
        let result = unsafe {
            self.low_latency
                .set_latency_sleep_mode(swapchain, Some(&info))
        };
        if let Err(e) = result {
            tracing::warn!("Reflex: failed to enable low-latency mode: {e:?}");
            return;
        }
        self.enabled = true;
    }

    /// Disable low-latency mode on the given swapchain.
    pub(crate) fn disable(&mut self, swapchain: vk::SwapchainKHR) {
        if !self.enabled {
            return;
        }
        let info = vk::LatencySleepModeInfoNV::default()
            .low_latency_mode(false)
            .low_latency_boost(false);
        let _ = unsafe {
            self.low_latency
                .set_latency_sleep_mode(swapchain, Some(&info))
        };
        self.enabled = false;
    }

    /// Whether Reflex low-latency mode is currently enabled.
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Increment the internal frame counter and return the new frame ID.
    pub(crate) fn begin_frame(&mut self) -> u64 {
        self.frame_id = self.frame_id.wrapping_add(1);
        self.frame_id
    }

    /// Current frame ID (set by the most recent `begin_frame` call).
    pub(crate) fn current_frame_id(&self) -> u64 {
        self.frame_id
    }

    /// Block the CPU until the driver is ready for the next frame.
    ///
    /// Call this at the very start of the render work (before swapchain acquire)
    /// so Reflex can pace the CPU and keep the render queue short.
    pub(crate) fn sleep(&self, swapchain: vk::SwapchainKHR) {
        if !self.enabled {
            return;
        }
        let info = vk::LatencySleepInfoNV::default()
            .signal_semaphore(self.sleep_semaphore)
            .value(self.frame_id);
        if let Err(e) = unsafe { self.low_latency.latency_sleep(swapchain, &info) } {
            tracing::warn!("Reflex: latency_sleep failed: {e:?}");
        }
    }

    /// Place a latency marker for the current frame.
    pub(crate) fn set_marker(&self, swapchain: vk::SwapchainKHR, marker: vk::LatencyMarkerNV) {
        if !self.enabled {
            return;
        }
        let info = vk::SetLatencyMarkerInfoNV::default()
            .present_id(self.frame_id)
            .marker(marker);
        unsafe {
            self.low_latency.set_latency_marker(swapchain, &info);
        }
    }
}

impl Drop for ReflexContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.raw_device.device_wait_idle();
            self.raw_device
                .destroy_semaphore(self.sleep_semaphore, None);
        }
    }
}

/// Helper to extract the raw `VkSwapchainKHR` from a wgpu surface.
///
/// Returns `None` if the surface is not configured or not using a native Vulkan swapchain.
pub(crate) fn raw_swapchain(surface: &wgpu::Surface<'_>) -> Option<vk::SwapchainKHR> {
    unsafe {
        surface
            .as_hal::<Vulkan>()
            .and_then(|s| s.raw_native_swapchain())
    }
}
