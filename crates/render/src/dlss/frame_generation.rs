use super::{DlssSdk, nvsdk_ngx::*};
use glam::{Mat4, UVec2};
use std::{
    iter, ptr,
    sync::{Arc, Mutex},
};
use wgpu::{
    Adapter, CommandBuffer, CommandEncoder, CommandEncoderDescriptor, Device, Queue, Texture,
    TextureTransition, TextureUses, TextureView, hal::api::Vulkan,
};

/// Context for DLSS Frame Generation (DLSS-G).
///
/// Creates interpolated frames from consecutive rendered frames, effectively
/// doubling the display framerate. Requires RTX 4000+ (Ada Lovelace) hardware.
pub struct DlssFrameGeneration {
    output_resolution: UVec2,
    device: Device,
    sdk: Arc<Mutex<DlssSdk>>,
    feature: *mut NVSDK_NGX_Handle,
}

impl DlssFrameGeneration {
    /// Create a new Frame Generation context.
    ///
    /// This is an expensive operation. The resulting object should be cached and
    /// only recreated when the output resolution changes.
    pub fn new(
        output_resolution: UVec2,
        sdk: Arc<Mutex<DlssSdk>>,
        device: &Device,
        queue: &Queue,
        adapter: &Adapter,
    ) -> Result<Self, DlssError> {
        let locked_sdk = sdk.lock().unwrap();

        // Resolve VkFormat for the backbuffer (Rgba8Unorm).
        let backbuffer_vk_format = unsafe {
            adapter
                .as_hal::<Vulkan>()
                .unwrap()
                .texture_format_as_raw(wgpu::TextureFormat::Rgba8Unorm)
        };

        // Set creation parameters manually (the DLSSG helpers use C++ and aren't bindgen-compatible).
        unsafe {
            let p = locked_sdk.parameters;
            NVSDK_NGX_Parameter_SetUI(p, NVSDK_NGX_Parameter_CreationNodeMask.as_ptr().cast(), 1);
            NVSDK_NGX_Parameter_SetUI(p, NVSDK_NGX_Parameter_VisibilityNodeMask.as_ptr().cast(), 1);
            // Use DLSSG-specific width/height (the generic ones are deprecated for FG).
            NVSDK_NGX_Parameter_SetUI(
                p,
                NVSDK_NGX_DLSSG_Parameter_Width.as_ptr().cast(),
                output_resolution.x,
            );
            NVSDK_NGX_Parameter_SetUI(
                p,
                NVSDK_NGX_DLSSG_Parameter_Height.as_ptr().cast(),
                output_resolution.y,
            );
            NVSDK_NGX_Parameter_SetUI(
                p,
                NVSDK_NGX_DLSSG_Parameter_BackbufferFormat.as_ptr().cast(),
                backbuffer_vk_format.as_raw() as u32,
            );
        }

        let mut command_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("dlss_frame_generation_context_creation"),
        });

        let mut feature = ptr::null_mut();
        unsafe {
            command_encoder.as_hal_mut::<Vulkan, _, _>(|command_encoder| {
                check_ngx_result(NVSDK_NGX_VULKAN_CreateFeature(
                    command_encoder.unwrap().raw_handle(),
                    NVSDK_NGX_Feature_NVSDK_NGX_Feature_FrameGeneration,
                    locked_sdk.parameters,
                    &mut feature,
                ))
            })?
        }

        queue.submit([command_encoder.finish()]);

        Ok(Self {
            output_resolution,
            device: device.clone(),
            sdk: Arc::clone(&sdk),
            feature,
        })
    }

    /// Evaluate Frame Generation — produce an interpolated frame.
    ///
    /// The returned command buffer must be submitted immediately after the
    /// finished `command_encoder`, in the same `queue.submit` call.
    pub fn evaluate(
        &mut self,
        params: DlssFrameGenerationEvalParams,
        command_encoder: &mut CommandEncoder,
        adapter: &Adapter,
    ) -> Result<CommandBuffer, DlssError> {
        let sdk = self.sdk.lock().unwrap();

        // Set evaluation parameters via the NGX parameter interface.
        let mut backbuffer_res = texture_to_ngx(params.backbuffer, adapter);
        let mut depth_res = texture_to_ngx(params.depth, adapter);
        let mut mvecs_res = texture_to_ngx(params.motion_vectors, adapter);
        let mut hudless_res = texture_to_ngx(params.hudless, adapter);
        let mut output_res = texture_to_ngx(params.output, adapter);
        let mut real_output_res = texture_to_ngx(params.real_output, adapter);

        // Compute clip-to-prev-clip and prev-clip-to-clip transforms.
        let cam = &params.camera;
        let cur = Mat4::from_cols_array(&cam.clip_from_world);
        let prev = Mat4::from_cols_array(&cam.prev_clip_from_world);
        let cur_inv = cur.inverse();
        let prev_inv = prev.inverse();
        let mut clip_to_prev_clip = (prev * cur_inv).to_cols_array();
        let mut prev_clip_to_clip = (cur * prev_inv).to_cols_array();
        let mut view_to_clip = cam.view_to_clip;

        unsafe {
            let p = sdk.parameters;

            // --- Texture resources ---
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_DLSSG_Parameter_Backbuffer.as_ptr().cast(),
                &mut backbuffer_res as *mut _ as *mut _,
            );
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_DLSSG_Parameter_Depth.as_ptr().cast(),
                &mut depth_res as *mut _ as *mut _,
            );
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_DLSSG_Parameter_MVecs.as_ptr().cast(),
                &mut mvecs_res as *mut _ as *mut _,
            );
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_DLSSG_Parameter_HUDLess.as_ptr().cast(),
                &mut hudless_res as *mut _ as *mut _,
            );
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_Parameter_OutputInterpolated.as_ptr().cast(),
                &mut output_res as *mut _ as *mut _,
            );
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_Parameter_OutputReal.as_ptr().cast(),
                &mut real_output_res as *mut _ as *mut _,
            );
            // Optional UI overlay — null (no HUD separation).
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_DLSSG_Parameter_UI.as_ptr().cast(),
                ptr::null_mut(),
            );

            // --- Camera matrices (column-major float[16] pointers) ---
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraViewToClip.as_ptr().cast(),
                view_to_clip.as_mut_ptr() as *mut _,
            );
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_DLSSG_Parameter_ClipToPrevClip.as_ptr().cast(),
                clip_to_prev_clip.as_mut_ptr() as *mut _,
            );
            NVSDK_NGX_Parameter_SetVoidPointer(
                p,
                NVSDK_NGX_DLSSG_Parameter_PrevClipToClip.as_ptr().cast(),
                prev_clip_to_clip.as_mut_ptr() as *mut _,
            );

            // --- Camera vectors and scalars ---
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraPosX.as_ptr().cast(),
                cam.position[0],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraPosY.as_ptr().cast(),
                cam.position[1],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraPosZ.as_ptr().cast(),
                cam.position[2],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraFwdX.as_ptr().cast(),
                cam.forward[0],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraFwdY.as_ptr().cast(),
                cam.forward[1],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraFwdZ.as_ptr().cast(),
                cam.forward[2],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraUpX.as_ptr().cast(),
                cam.up[0],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraUpY.as_ptr().cast(),
                cam.up[1],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraUpZ.as_ptr().cast(),
                cam.up[2],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraRightX.as_ptr().cast(),
                cam.right[0],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraRightY.as_ptr().cast(),
                cam.right[1],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraRightZ.as_ptr().cast(),
                cam.right[2],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraNear.as_ptr().cast(),
                cam.near,
            );
            NVSDK_NGX_Parameter_SetF(p, NVSDK_NGX_DLSSG_Parameter_CameraFar.as_ptr().cast(), 0.0); // infinite far
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraFOV.as_ptr().cast(),
                cam.fov_y,
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraAspectRatio.as_ptr().cast(),
                cam.aspect,
            );

            // --- Jitter ---
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_JitterOffsetX.as_ptr().cast(),
                cam.jitter[0],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_JitterOffsetY.as_ptr().cast(),
                cam.jitter[1],
            );

            // --- Motion vector interpretation ---
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_MvecScaleX.as_ptr().cast(),
                cam.mvec_scale[0],
            );
            NVSDK_NGX_Parameter_SetF(
                p,
                NVSDK_NGX_DLSSG_Parameter_MvecScaleY.as_ptr().cast(),
                cam.mvec_scale[1],
            );
            NVSDK_NGX_Parameter_SetI(
                p,
                NVSDK_NGX_DLSSG_Parameter_DepthInverted.as_ptr().cast(),
                i32::from(cam.depth_inverted),
            );
            NVSDK_NGX_Parameter_SetI(
                p,
                NVSDK_NGX_DLSSG_Parameter_CameraMotionIncluded
                    .as_ptr()
                    .cast(),
                i32::from(cam.camera_motion_included),
            );

            // --- Temporal reset ---
            NVSDK_NGX_Parameter_SetI(
                p,
                NVSDK_NGX_DLSSG_Parameter_Reset.as_ptr().cast(),
                i32::from(params.reset),
            );
        }

        tracing::debug!(
            reset = params.reset,
            output_w = params.output.texture().width(),
            output_h = params.output.texture().height(),
            backbuffer_w = params.backbuffer.texture().width(),
            backbuffer_h = params.backbuffer.texture().height(),
            depth_inverted = cam.depth_inverted,
            mvec_scale_x = cam.mvec_scale[0],
            mvec_scale_y = cam.mvec_scale[1],
            "FG evaluate: setting NGX params"
        );

        command_encoder.transition_resources(iter::empty(), params.barrier_list());

        // Get raw VkImages for both output textures.
        let output_image = unsafe {
            params
                .output
                .texture()
                .as_hal::<Vulkan>()
                .unwrap()
                .raw_handle()
        };
        let real_output_image = unsafe {
            params
                .real_output
                .texture()
                .as_hal::<Vulkan>()
                .unwrap()
                .raw_handle()
        };

        let mut fg_command_encoder =
            self.device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("dlss_frame_generation"),
                });
        unsafe {
            let hal_device = self.device.as_hal::<Vulkan>().unwrap();
            let raw_device = hal_device.raw_device();

            fg_command_encoder.as_hal_mut::<Vulkan, _, _>(|command_encoder| {
                let raw_cb = command_encoder.unwrap().raw_handle();

                let subresource_range = ash::vk::ImageSubresourceRange {
                    aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: ash::vk::REMAINING_MIP_LEVELS,
                    base_array_layer: 0,
                    layer_count: ash::vk::REMAINING_ARRAY_LAYERS,
                };
                // Barriers for both FG output textures.
                let barriers = [
                    ash::vk::ImageMemoryBarrier::default()
                        .src_access_mask(ash::vk::AccessFlags::MEMORY_WRITE)
                        .dst_access_mask(
                            ash::vk::AccessFlags::TRANSFER_WRITE
                                | ash::vk::AccessFlags::SHADER_READ
                                | ash::vk::AccessFlags::SHADER_WRITE,
                        )
                        .old_layout(ash::vk::ImageLayout::GENERAL)
                        .new_layout(ash::vk::ImageLayout::GENERAL)
                        .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                        .image(output_image)
                        .subresource_range(subresource_range),
                    ash::vk::ImageMemoryBarrier::default()
                        .src_access_mask(ash::vk::AccessFlags::MEMORY_WRITE)
                        .dst_access_mask(
                            ash::vk::AccessFlags::TRANSFER_WRITE
                                | ash::vk::AccessFlags::SHADER_READ
                                | ash::vk::AccessFlags::SHADER_WRITE,
                        )
                        .old_layout(ash::vk::ImageLayout::GENERAL)
                        .new_layout(ash::vk::ImageLayout::GENERAL)
                        .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                        .image(real_output_image)
                        .subresource_range(subresource_range),
                ];
                raw_device.cmd_pipeline_barrier(
                    raw_cb,
                    ash::vk::PipelineStageFlags::ALL_COMMANDS,
                    ash::vk::PipelineStageFlags::TRANSFER
                        | ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                    ash::vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );

                let result = check_ngx_result(NVSDK_NGX_VULKAN_EvaluateFeature_C(
                    raw_cb,
                    self.feature,
                    sdk.parameters,
                    None, // No PFN callback
                ));
                tracing::debug!(?result, "FG EvaluateFeature_C returned");
                result
            })?;
        }
        Ok(fg_command_encoder.finish())
    }

    /// The resolution Frame Generation outputs at.
    pub fn output_resolution(&self) -> UVec2 {
        self.output_resolution
    }
}

impl Drop for DlssFrameGeneration {
    fn drop(&mut self) {
        unsafe {
            let hal_device = self.device.as_hal::<Vulkan>().unwrap();
            hal_device
                .raw_device()
                .device_wait_idle()
                .expect("Failed to wait for idle device when destroying DlssFrameGeneration");

            check_ngx_result(NVSDK_NGX_VULKAN_ReleaseFeature(self.feature))
                .expect("Failed to destroy DlssFrameGeneration feature");
        }
    }
}

unsafe impl Send for DlssFrameGeneration {}
unsafe impl Sync for DlssFrameGeneration {}

/// Camera and scene metadata required by Frame Generation.
pub struct FgCameraParams {
    /// Current frame's view-to-clip (projection) matrix — column-major `[f32; 16]`.
    pub view_to_clip: [f32; 16],
    /// Current frame's clip_from_world (projection * view) — column-major.
    pub clip_from_world: [f32; 16],
    /// Previous frame's clip_from_world — column-major.
    pub prev_clip_from_world: [f32; 16],
    /// Camera world-space position.
    pub position: [f32; 3],
    /// Camera forward direction (normalised).
    pub forward: [f32; 3],
    /// Camera up direction (normalised).
    pub up: [f32; 3],
    /// Camera right direction (normalised).
    pub right: [f32; 3],
    /// Near clip plane distance.
    pub near: f32,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Aspect ratio (width / height).
    pub aspect: f32,
    /// Temporal jitter offset (in pixels).
    pub jitter: [f32; 2],
    /// Motion-vector → pixel scale per axis.
    pub mvec_scale: [f32; 2],
    /// `true` when the depth buffer uses reversed-Z.
    pub depth_inverted: bool,
    /// `true` when motion vectors include camera motion (not just object motion).
    pub camera_motion_included: bool,
}

/// Inputs and output resources needed for evaluating Frame Generation.
pub struct DlssFrameGenerationEvalParams<'a> {
    /// Current upscaled frame (output of SR/RR/FSR).
    pub backbuffer: &'a TextureView,
    /// HUD-less version of the backbuffer (same as backbuffer if no HUD separation).
    pub hudless: &'a TextureView,
    /// Depth buffer.
    pub depth: &'a TextureView,
    /// Motion vectors.
    pub motion_vectors: &'a TextureView,
    /// Output texture — the interpolated frame will be written here.
    pub output: &'a TextureView,
    /// Separate output for the real frame (must differ from backbuffer).
    pub real_output: &'a TextureView,
    /// Whether Frame Generation should reset its temporal state.
    pub reset: bool,
    /// Camera and scene metadata for interpolation.
    pub camera: FgCameraParams,
}

impl<'a> DlssFrameGenerationEvalParams<'a> {
    fn barrier_list(&self) -> impl Iterator<Item = TextureTransition<&'a Texture>> {
        fn resource_barrier<'a>(texture_view: &'a TextureView) -> TextureTransition<&'a Texture> {
            TextureTransition {
                texture: texture_view.texture(),
                selector: None,
                state: TextureUses::RESOURCE,
            }
        }

        [
            Some(resource_barrier(self.backbuffer)),
            // HUDLess points to the same texture as backbuffer — skip duplicate.
            None,
            Some(resource_barrier(self.depth)),
            Some(resource_barrier(self.motion_vectors)),
            Some(TextureTransition {
                texture: self.output.texture(),
                selector: None,
                state: TextureUses::STORAGE_READ_WRITE,
            }),
            Some(TextureTransition {
                texture: self.real_output.texture(),
                selector: None,
                state: TextureUses::STORAGE_READ_WRITE,
            }),
        ]
        .into_iter()
        .flatten()
    }
}
