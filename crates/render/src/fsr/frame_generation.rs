//! FSR 3 Frame Generation — DX12-based.
//!
//! Uses the FidelityFX unified API to generate interpolated frames from
//! consecutive rendered frames. Mirrors the DLSS Frame Generation double-present
//! pattern: the caller blits the interpolated output to the current swapchain
//! texture, then acquires a new texture for the real frame.

use glam::{UVec2, Vec2};
use wgpu::{
    CommandEncoder, CommandEncoderDescriptor, Device, TextureTransition, TextureUses, TextureView,
    hal::api::Dx12,
};
use windows::Win32::Graphics::Direct3D12;
use windows_core::Interface as _;

use super::FsrError;
use super::fidelityfx as ffx;

/// Camera and scene metadata required by FSR Frame Generation.
pub(crate) struct FsrFgCameraParams {
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
}

/// Wraps the FidelityFX frame-generation context and manages its lifetime.
pub(crate) struct FsrFrameGeneration {
    context: ffx::ffxContext,
    device: Device,
    /// Raw DX12 device for command list creation each frame.
    dx12_device: Direct3D12::ID3D12Device,
    /// Raw DX12 queue for executing FG command lists.
    dx12_queue: Direct3D12::ID3D12CommandQueue,
    /// Reusable command allocator — reset once the GPU signals it's done.
    allocator: Direct3D12::ID3D12CommandAllocator,
    /// Fence used to know when the previous FG dispatch has finished.
    fence: Direct3D12::ID3D12Fence,
    /// Monotonically increasing value signalled after each dispatch.
    fence_value: u64,
    display_resolution: UVec2,
    render_resolution: UVec2,
    /// Frame identifier — must increment by exactly one each frame for FSR FG.
    frame_id: u64,
}

impl FsrFrameGeneration {
    /// Create a new FSR 3 Frame Generation context.
    ///
    /// `display_size` is the output (display) resolution; `render_size` is the
    /// lower internal resolution the game renders at.
    pub(crate) fn new(
        device: &Device,
        queue: &wgpu::Queue,
        adapter: &wgpu::Adapter,
        display_size: UVec2,
        render_size: UVec2,
    ) -> Result<Self, FsrError> {
        tracing::info!("FsrFrameGeneration::new — display={display_size}, render={render_size}");

        if adapter.get_info().backend != wgpu::Backend::Dx12 {
            return Err(FsrError::NotDx12);
        }

        // Extract raw DX12 handles from wgpu's HAL.
        let dx12_device: Direct3D12::ID3D12Device = unsafe {
            device
                .as_hal::<Dx12>()
                .map(|hal_device| hal_device.raw_device().clone())
                .ok_or(FsrError::HalAccess)?
        };
        let dx12_queue: Direct3D12::ID3D12CommandQueue = unsafe {
            queue
                .as_hal::<Dx12>()
                .map(|hal_queue| hal_queue.as_raw().clone())
                .ok_or(FsrError::HalAccess)?
        };

        let raw_device = dx12_device.as_raw() as *mut core::ffi::c_void;

        let allocator: Direct3D12::ID3D12CommandAllocator = unsafe {
            dx12_device
                .CreateCommandAllocator(Direct3D12::D3D12_COMMAND_LIST_TYPE_DIRECT)
                .map_err(|_| FsrError::ContextCreation)?
        };
        let fence: Direct3D12::ID3D12Fence = unsafe {
            dx12_device
                .CreateFence(0, Direct3D12::D3D12_FENCE_FLAG_NONE)
                .map_err(|_| FsrError::ContextCreation)?
        };

        // --- Build descriptor chain: FG → Version → Backend ---

        let mut backend_desc = ffx::ffxCreateBackendDX12Desc {
            header: ffx::ffxApiHeader {
                type_: ffx::FFX_API_CREATE_CONTEXT_DESC_TYPE_BACKEND_DX12 as u64,
                pNext: core::ptr::null_mut(),
            },
            device: raw_device,
        };

        // Version descriptor — required to enable V2 APIs (PrepareV2).
        let mut version_desc: ffx::ffxCreateContextDescFrameGenerationVersion =
            unsafe { core::mem::zeroed() };
        version_desc.header.type_ = ffx::FFX_API_CREATE_CONTEXT_DESC_TYPE_FRAMEGENERATION_VERSION;
        version_desc.header.pNext =
            &mut backend_desc.header as *mut ffx::ffxCreateContextDescHeader;
        version_desc.version = ffx::FFX_FRAMEGENERATION_VERSION;

        // Frame generation creation descriptor.
        let mut fg_desc: ffx::ffxCreateContextDescFrameGeneration = unsafe { core::mem::zeroed() };
        fg_desc.header.type_ = ffx::FFX_API_CREATE_CONTEXT_DESC_TYPE_FRAMEGENERATION;
        fg_desc.header.pNext = &mut version_desc.header as *mut ffx::ffxCreateContextDescHeader;
        fg_desc.displaySize = ffx::FfxApiDimensions2D {
            width: display_size.x,
            height: display_size.y,
        };
        fg_desc.maxRenderSize = ffx::FfxApiDimensions2D {
            width: render_size.x,
            height: render_size.y,
        };
        fg_desc.backBufferFormat = ffx::FFX_API_SURFACE_FORMAT_R8G8B8A8_UNORM;

        let mut context: ffx::ffxContext = core::ptr::null_mut();
        unsafe {
            ffx::create_context(
                &mut context,
                &mut fg_desc.header as *mut ffx::ffxCreateContextDescHeader,
            )
            .map_err(|e| {
                tracing::error!("FSR FG: ffxCreateContext failed: {e:?}");
                FsrError::ContextCreation
            })?;
        }
        tracing::info!("FSR FG: context created successfully");

        Ok(Self {
            context,
            device: device.clone(),
            dx12_device,
            dx12_queue,
            allocator,
            fence,
            fence_value: 0,
            display_resolution: display_size,
            render_resolution: render_size,
            frame_id: 0,
        })
    }

    /// Evaluate Frame Generation — produce an interpolated frame.
    ///
    /// Returns `true` when an interpolated frame was written to `output`.
    /// The first call after creation only runs the optical-flow prepare step
    /// (building frame history); interpolation begins on the second call.
    ///
    /// This method:
    /// 1. Records resource transitions into `encoder`.
    /// 2. Submits `encoder` to `queue` so transitions are applied.
    /// 3. Configures the FG context for this frame.
    /// 4. Dispatches optical-flow preparation (every frame) and frame
    ///    interpolation (frame 1+) on the raw DX12 queue.
    /// 5. Replaces `encoder` with a fresh one for subsequent work.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn evaluate(
        &mut self,
        encoder: &mut CommandEncoder,
        queue: &wgpu::Queue,
        present_color: &TextureView,
        depth: &TextureView,
        motion_vectors: &TextureView,
        output: &TextureView,
        reset: bool,
        jitter: Vec2,
        render_size: UVec2,
        delta_time_ms: f32,
        camera: &FsrFgCameraParams,
    ) -> Result<bool, FsrError> {
        // Transition inputs to RESOURCE and output to STORAGE_READ_WRITE.
        encoder.transition_resources(
            core::iter::empty(),
            [
                TextureTransition {
                    texture: present_color.texture(),
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

        // Submit encoder so transitions are applied before FG work.
        let submitted = std::mem::replace(
            encoder,
            self.device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("Post-FSR-FG"),
                }),
        );
        queue.submit(std::iter::once(submitted.finish()));

        let produced_frame;
        unsafe {
            // Wait for the previous FG dispatch to finish so the command
            // allocator can safely be reset and reused.
            self.wait_for_gpu()?;
            self.allocator.Reset().map_err(|_| FsrError::Dispatch)?;

            let cmd_list: Direct3D12::ID3D12GraphicsCommandList = self
                .dx12_device
                .CreateCommandList(
                    0,
                    Direct3D12::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &self.allocator,
                    None,
                )
                .map_err(|_| FsrError::Dispatch)?;

            let cmd_list_ptr = cmd_list.as_raw() as *mut core::ffi::c_void;

            // --- Configure: enable FG and set frame ID for this tick ---
            self.configure_frame_inner()?;

            // --- Prepare: optical flow from depth + motion vectors ---
            let fsr_depth = self.texture_resource(
                depth,
                render_size,
                ffx::FfxApiResourceState_FFX_API_RESOURCE_STATE_COMPUTE_READ,
            )?;
            let fsr_motion = self.texture_resource(
                motion_vectors,
                render_size,
                ffx::FfxApiResourceState_FFX_API_RESOURCE_STATE_COMPUTE_READ,
            )?;

            let mut prepare_desc: ffx::ffxDispatchDescFrameGenerationPrepareV2 =
                core::mem::zeroed();
            prepare_desc.header.type_ = ffx::FFX_API_DISPATCH_DESC_TYPE_FRAMEGENERATION_PREPARE_V2;
            prepare_desc.commandList = cmd_list_ptr;
            prepare_desc.frameID = self.frame_id;
            prepare_desc.renderSize = ffx::FfxApiDimensions2D {
                width: render_size.x,
                height: render_size.y,
            };
            prepare_desc.jitterOffset = ffx::FfxApiFloatCoords2D {
                x: jitter.x,
                y: jitter.y,
            };
            prepare_desc.motionVectorScale = ffx::FfxApiFloatCoords2D {
                x: -(render_size.x as f32),
                y: -(render_size.y as f32),
            };
            prepare_desc.frameTimeDelta = delta_time_ms;
            prepare_desc.reset = reset;
            prepare_desc.cameraNear = camera.near;
            prepare_desc.cameraFar = 0.0; // infinite far plane
            prepare_desc.cameraFovAngleVertical = camera.fov_y;
            prepare_desc.viewSpaceToMetersFactor = 1.0;
            prepare_desc.depth = fsr_depth;
            prepare_desc.motionVectors = fsr_motion;
            prepare_desc.cameraPosition = camera.position;
            prepare_desc.cameraUp = camera.up;
            prepare_desc.cameraRight = camera.right;
            prepare_desc.cameraForward = camera.forward;

            ffx::dispatch(
                &mut self.context,
                &prepare_desc.header as *const _ as *const ffx::ffxDispatchDescHeader,
            )
            .map_err(|e| {
                tracing::error!("FSR FG: prepare dispatch failed: {e:?}");
                FsrError::Dispatch
            })?;

            // --- Frame Generation: interpolate from presentColor to output ---
            // The first prepare builds optical-flow history; interpolation
            // requires at least one previous frame, so skip the FG dispatch
            // on the very first call.
            produced_frame = if self.frame_id > 0 {
                let fsr_present = self.texture_resource(
                    present_color,
                    self.display_resolution,
                    ffx::FfxApiResourceState_FFX_API_RESOURCE_STATE_COMPUTE_READ,
                )?;
                let fsr_output = self.texture_resource(
                    output,
                    self.display_resolution,
                    ffx::FfxApiResourceState_FFX_API_RESOURCE_STATE_UNORDERED_ACCESS,
                )?;

                let mut fg_desc: ffx::ffxDispatchDescFrameGeneration = core::mem::zeroed();
                fg_desc.header.type_ = ffx::FFX_API_DISPATCH_DESC_TYPE_FRAMEGENERATION;
                fg_desc.commandList = cmd_list_ptr;
                fg_desc.presentColor = fsr_present;
                fg_desc.outputs[0] = fsr_output;
                fg_desc.numGeneratedFrames = 1;
                fg_desc.reset = reset;
                fg_desc.generationRect = ffx::FfxApiRect2D {
                    left: 0,
                    top: 0,
                    width: self.display_resolution.x as i32,
                    height: self.display_resolution.y as i32,
                };
                fg_desc.frameID = self.frame_id;

                ffx::dispatch(
                    &mut self.context,
                    &fg_desc.header as *const _ as *const ffx::ffxDispatchDescHeader,
                )
                .map_err(|e| {
                    tracing::error!("FSR FG: frame generation dispatch failed: {e:?}");
                    FsrError::Dispatch
                })?;

                true
            } else {
                tracing::debug!("FSR FG: first frame — prepare only, skipping interpolation");
                false
            };

            cmd_list.Close().map_err(|_| FsrError::Dispatch)?;

            // Execute the FG command list on the DX12 queue.
            self.dx12_queue
                .ExecuteCommandLists(&[Some(cmd_list.cast().map_err(|_| FsrError::Dispatch)?)]);

            // Signal the fence so the next frame knows when this dispatch is done.
            self.fence_value += 1;
            self.dx12_queue
                .Signal(&self.fence, self.fence_value)
                .map_err(|_| FsrError::Dispatch)?;
        }

        self.frame_id += 1;

        tracing::debug!(
            "FSR FG: frame {} evaluated, produced={produced_frame} (display={}x{})",
            self.frame_id - 1,
            self.display_resolution.x,
            self.display_resolution.y,
        );

        Ok(produced_frame)
    }

    /// CPU-side configure call: enable frame generation and update the frame ID.
    unsafe fn configure_frame_inner(&mut self) -> Result<(), FsrError> {
        let mut desc: ffx::ffxConfigureDescFrameGeneration = core::mem::zeroed();
        desc.header.type_ = ffx::FFX_API_CONFIGURE_DESC_TYPE_FRAMEGENERATION;
        desc.frameGenerationEnabled = true;
        // Manual mode — we handle present/double-present ourselves.
        desc.flags = ffx::FFX_FRAMEGENERATION_FLAG_NO_SWAPCHAIN_CONTEXT_NOTIFY;
        desc.generationRect = ffx::FfxApiRect2D {
            left: 0,
            top: 0,
            width: self.display_resolution.x as i32,
            height: self.display_resolution.y as i32,
        };
        desc.frameID = self.frame_id;

        ffx::configure(
            &mut self.context,
            &desc.header as *const ffx::ffxConfigureDescHeader,
        )
        .map_err(|e| {
            tracing::error!("FSR FG: ffxConfigure failed: {e:?}");
            FsrError::Dispatch
        })
    }

    /// Block the CPU until the GPU has finished the most recent FG dispatch.
    unsafe fn wait_for_gpu(&self) -> Result<(), FsrError> {
        if self.fence_value == 0 {
            return Ok(());
        }
        let completed = self.fence.GetCompletedValue();
        if completed < self.fence_value {
            use windows::Win32::System::Threading;

            let event = Threading::CreateEventW(None, false, false, None)
                .map_err(|_| FsrError::Dispatch)?;
            self.fence
                .SetEventOnCompletion(self.fence_value, event)
                .map_err(|_| FsrError::Dispatch)?;
            Threading::WaitForSingleObject(event, Threading::INFINITE);
            let _ = windows::Win32::Foundation::CloseHandle(event);
        }
        Ok(())
    }

    /// Convert a wgpu `TextureView` into an `FfxApiResource` by extracting
    /// the raw DX12 `ID3D12Resource` pointer.
    fn texture_resource(
        &self,
        view: &TextureView,
        size: UVec2,
        state: ffx::FfxApiResourceState,
    ) -> Result<ffx::FfxApiResource, FsrError> {
        unsafe {
            let texture = view.texture();
            let hal_texture = texture.as_hal::<Dx12>().ok_or(FsrError::HalAccess)?;
            let raw_resource = hal_texture.raw_resource();

            let mut resource: ffx::FfxApiResource = core::mem::zeroed();
            resource.resource = raw_resource.as_raw() as *mut core::ffi::c_void;
            resource.description.type_ =
                ffx::FfxApiResourceType_FFX_API_RESOURCE_TYPE_TEXTURE2D as u32;
            resource.description.format = super::wgpu_to_ffx_format(texture.format());
            resource.description.__bindgen_anon_1.width = size.x;
            resource.description.__bindgen_anon_2.height = size.y;
            resource.description.__bindgen_anon_3.depth = 1;
            resource.description.mipCount = 1;
            resource.state = state as u32;
            Ok(resource)
        }
    }
}

impl Drop for FsrFrameGeneration {
    fn drop(&mut self) {
        let _ = unsafe { self.wait_for_gpu() };
        let _ = unsafe { ffx::destroy_context(&mut self.context) };
    }
}

// SAFETY: Same as FsrContext — render-thread only via NonSend.
unsafe impl Send for FsrFrameGeneration {}
unsafe impl Sync for FsrFrameGeneration {}
