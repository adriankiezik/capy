//! FSR 2 context creation and dispatch.

use std::iter;

use ash::vk;
use glam::{UVec2, Vec2};
use wgpu::{
    Adapter, CommandBuffer, CommandEncoder, CommandEncoderDescriptor, Device, TextureTransition,
    TextureUses, TextureView, hal::api::Vulkan,
};

use super::FsrError;

/// Wraps the FSR 2 context and manages its lifetime.
pub(crate) struct FsrContext {
    context: fsr::Context,
    device: Device,
    render_resolution: UVec2,
    display_resolution: UVec2,
}

impl FsrContext {
    /// Create a new FSR 2 context.
    ///
    /// `display_size` is the output resolution; `render_size` is the lower internal resolution.
    pub(crate) fn new(
        device: &Device,
        adapter: &Adapter,
        display_size: UVec2,
        render_size: UVec2,
        depth_inverted: bool,
    ) -> Result<Self, FsrError> {
        if adapter.get_info().backend != wgpu::Backend::Vulkan {
            return Err(FsrError::NotVulkan);
        }

        // Extract raw Vulkan handles from wgpu via HAL.
        let interface = unsafe {
            adapter.as_hal::<Vulkan>().map(|hal_adapter| {
                let raw_instance = hal_adapter.shared_instance().raw_instance();
                let entry = hal_adapter.shared_instance().entry();
                let physical_device = hal_adapter.raw_physical_device();
                fsr::vk::get_interface(entry, raw_instance, physical_device)
            })
        };
        let interface = interface.ok_or(FsrError::HalAccess)??;

        let fsr_device = unsafe {
            device
                .as_hal::<Vulkan>()
                .map(|hal_device| fsr::vk::get_device(hal_device.raw_device().clone()))
        };
        let fsr_device = fsr_device.ok_or(FsrError::HalAccess)?;

        let mut flags = fsr::InitializationFlagBits::ENABLE_AUTO_EXPOSURE;
        if depth_inverted {
            flags |= fsr::InitializationFlagBits::ENABLE_DEPTH_INVERTED;
        }

        let context_desc = fsr::ContextDescription {
            interface,
            device: &fsr_device,
            display_size: display_size.to_array(),
            max_render_size: render_size.to_array(),
            flags,
            message_callback: None,
        };

        let context = unsafe { fsr::Context::new(context_desc)? };

        Ok(Self {
            context,
            device: device.clone(),
            render_resolution: render_size,
            display_resolution: display_size,
        })
    }

    pub(crate) fn render_resolution(&self) -> UVec2 {
        self.render_resolution
    }

    /// Compute the suggested jitter offset for the given frame index.
    pub(crate) fn suggested_jitter(&self, frame_index: u32) -> Vec2 {
        let phase_count = unsafe {
            fsr_sys::GetJitterPhaseCount(
                self.render_resolution.x as i32,
                self.display_resolution.x as i32,
            )
        };
        let mut jitter_x = 0.0f32;
        let mut jitter_y = 0.0f32;
        unsafe {
            fsr_sys::GetJitterOffset(
                &mut jitter_x,
                &mut jitter_y,
                frame_index as i32,
                phase_count,
            );
        }
        Vec2::new(jitter_x, jitter_y)
    }

    /// Encode FSR 2 dispatch commands.
    ///
    /// Returns a command buffer that must be submitted immediately after the
    /// finished `command_encoder`, in the same queue submit call.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render(
        &mut self,
        command_encoder: &mut CommandEncoder,
        adapter: &Adapter,
        color: &TextureView,
        depth: &TextureView,
        motion_vectors: &TextureView,
        output: &TextureView,
        reset: bool,
        jitter: Vec2,
        render_size: UVec2,
        delta_time_ms: f32,
    ) -> Result<CommandBuffer, FsrError> {
        // Transition input textures to RESOURCE and output to STORAGE_READ_WRITE.
        command_encoder.transition_resources(
            iter::empty(),
            [
                TextureTransition {
                    texture: color.texture(),
                    selector: None,
                    state: TextureUses::RESOURCE,
                },
                TextureTransition {
                    texture: depth.texture(),
                    selector: None,
                    state: TextureUses::RESOURCE,
                },
                TextureTransition {
                    texture: motion_vectors.texture(),
                    selector: None,
                    state: TextureUses::RESOURCE,
                },
                TextureTransition {
                    texture: output.texture(),
                    selector: None,
                    state: TextureUses::STORAGE_READ_WRITE,
                },
            ]
            .into_iter(),
        );

        let output_image = unsafe {
            output
                .texture()
                .as_hal::<Vulkan>()
                .ok_or(FsrError::HalAccess)?
                .raw_handle()
        };

        let mut fsr_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("FSR 2 Dispatch"),
            });

        unsafe {
            fsr_encoder.as_hal_mut::<Vulkan, _, _>(|hal_encoder| -> Result<(), FsrError> {
                let raw_cb = hal_encoder.ok_or(FsrError::HalAccess)?.raw_handle();

                let fsr_color = self.texture_resource(color, adapter, render_size, "fsr_color")?;
                let fsr_depth = self.texture_resource(depth, adapter, render_size, "fsr_depth")?;
                let fsr_motion =
                    self.texture_resource(motion_vectors, adapter, render_size, "fsr_motion")?;
                let fsr_output =
                    self.texture_resource(output, adapter, self.display_resolution, "fsr_output")?;

                // Insert a barrier on the output image to ensure layout transition
                // is visible to FSR's internal writes.
                let hal_device = self.device.as_hal::<Vulkan>().ok_or(FsrError::HalAccess)?;
                let raw_device = hal_device.raw_device();
                let image_barrier = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(
                        vk::AccessFlags::SHADER_READ
                            | vk::AccessFlags::SHADER_WRITE
                            | vk::AccessFlags::TRANSFER_WRITE,
                    )
                    .old_layout(vk::ImageLayout::GENERAL)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(output_image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: vk::REMAINING_MIP_LEVELS,
                        base_array_layer: 0,
                        layer_count: vk::REMAINING_ARRAY_LAYERS,
                    });
                raw_device.cmd_pipeline_barrier(
                    raw_cb,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    std::slice::from_ref(&image_barrier),
                );

                let mut desc = fsr::DispatchDescription::new(
                    raw_cb.into(),
                    fsr_color,
                    fsr_depth,
                    fsr_motion,
                    fsr_output,
                    delta_time_ms,
                    render_size.to_array(),
                )
                .jitter_offset([jitter.x, jitter.y])
                .motion_vector_scale([-(render_size.x as f32), -(render_size.y as f32)]);

                if reset {
                    desc = desc.reset(true);
                }

                self.context.dispatch(desc).map_err(FsrError::from)
            })?;
        }

        Ok(fsr_encoder.finish())
    }

    /// Convert a wgpu `TextureView` to an FSR `Resource`.
    fn texture_resource(
        &mut self,
        view: &TextureView,
        adapter: &Adapter,
        size: UVec2,
        name: &str,
    ) -> Result<fsr::Resource, FsrError> {
        unsafe {
            let raw_view = view
                .as_hal::<Vulkan>()
                .ok_or(FsrError::HalAccess)?
                .raw_handle();
            let texture = view.texture();
            let raw_image = texture
                .as_hal::<Vulkan>()
                .ok_or(FsrError::HalAccess)?
                .raw_handle();

            let vk_format = adapter
                .as_hal::<Vulkan>()
                .ok_or(FsrError::HalAccess)?
                .texture_format_as_raw(texture.format());

            Ok(fsr::vk::get_texture_resource(
                &mut self.context,
                raw_image,
                raw_view,
                vk_format,
                size.to_array(),
                fsr::ResourceStates::COMPUTE_READ,
                name,
            ))
        }
    }
}

// SAFETY: The FSR context is only accessed from the render thread via NonSend.
// The context internally synchronizes through command buffer submission.
unsafe impl Send for FsrContext {}
unsafe impl Sync for FsrContext {}
