//! FSR 3.1 upscaler context — DX12-based.
//!
//! Extracts raw DX12 handles from wgpu and drives the FidelityFX upscaler
//! through the unified `ffx_api` C interface.

use glam::{UVec2, Vec2};
use wgpu::{
    Adapter, CommandEncoder, CommandEncoderDescriptor, Device, TextureTransition, TextureUses,
    TextureView, hal::api::Dx12,
};
use windows::Win32::Graphics::Direct3D12;
use windows_core::Interface as _;

use super::FsrError;
use super::fidelityfx as ffx;

/// Wraps the FidelityFX upscaler context and manages its lifetime.
pub(crate) struct FsrContext {
    context: ffx::ffxContext,
    device: Device,
    /// Raw DX12 device for command list creation each frame.
    dx12_device: Direct3D12::ID3D12Device,
    /// Raw DX12 queue for executing FSR command lists.
    dx12_queue: Direct3D12::ID3D12CommandQueue,
    render_resolution: UVec2,
    display_resolution: UVec2,
    /// Reusable command allocator — reset once the GPU signals it's done.
    allocator: Direct3D12::ID3D12CommandAllocator,
    /// Fence used to know when the previous FSR dispatch has finished on the GPU
    /// so that the command allocator can safely be reset.
    fence: Direct3D12::ID3D12Fence,
    /// Monotonically increasing value signalled after each dispatch.
    fence_value: u64,
}

impl FsrContext {
    /// Create a new FSR 3.1 upscaler context.
    ///
    /// `display_size` is the output (upscaled) resolution; `render_size` is
    /// the lower internal resolution the game renders at.
    pub(crate) fn new(
        device: &Device,
        queue: &wgpu::Queue,
        adapter: &Adapter,
        display_size: UVec2,
        render_size: UVec2,
        _depth_inverted: bool,
    ) -> Result<Self, FsrError> {
        tracing::debug!("FsrContext::new — display={display_size}, render={render_size}");

        if adapter.get_info().backend != wgpu::Backend::Dx12 {
            return Err(FsrError::NotDx12);
        }

        // Extract the raw ID3D12Device from wgpu's DX12 HAL.
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
        tracing::debug!("  DX12 device ptr: {raw_device:?}");

        // Create a reusable command allocator and a fence for synchronisation.
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

        // Build the DX12 backend descriptor.
        let mut backend_desc = ffx::ffxCreateBackendDX12Desc {
            header: ffx::ffxApiHeader {
                type_: ffx::FFX_API_CREATE_CONTEXT_DESC_TYPE_BACKEND_DX12 as u64,
                pNext: core::ptr::null_mut(),
            },
            device: raw_device,
        };

        // Build the upscaler creation descriptor.
        let mut upscale_desc: ffx::ffxCreateContextDescUpscale = unsafe { core::mem::zeroed() };
        upscale_desc.header.type_ = ffx::FFX_API_CREATE_CONTEXT_DESC_TYPE_UPSCALE;
        upscale_desc.maxRenderSize = ffx::FfxApiDimensions2D {
            width: render_size.x,
            height: render_size.y,
        };
        upscale_desc.maxUpscaleSize = ffx::FfxApiDimensions2D {
            width: display_size.x,
            height: display_size.y,
        };
        // Auto-exposure flag — bit 0 per the SDK.
        upscale_desc.flags = 1;

        // Chain: upscale → backend
        upscale_desc.header.pNext =
            &mut backend_desc.header as *mut ffx::ffxCreateContextDescHeader;

        let mut context: ffx::ffxContext = core::ptr::null_mut();
        tracing::debug!("  calling ffxCreateContext…");
        unsafe {
            ffx::create_context(
                &mut context,
                &mut upscale_desc.header as *mut ffx::ffxCreateContextDescHeader,
            )
            .map_err(|e| {
                tracing::error!("  ffxCreateContext failed: {e:?}");
                FsrError::ContextCreation
            })?;
        }
        tracing::debug!("  ffxCreateContext succeeded");

        Ok(Self {
            context,
            device: device.clone(),
            dx12_device,
            dx12_queue,
            render_resolution: render_size,
            display_resolution: display_size,
            allocator,
            fence,
            fence_value: 0,
        })
    }

    pub(crate) fn render_resolution(&self) -> UVec2 {
        self.render_resolution
    }

    /// Compute the suggested jitter offset for the given frame index.
    pub(crate) fn suggested_jitter(&mut self, frame_index: u32) -> Vec2 {
        // Query phase count.
        let mut phase_count: i32 = 0;
        let mut phase_query: ffx::ffxQueryDescUpscaleGetJitterPhaseCount =
            unsafe { core::mem::zeroed() };
        phase_query.header.type_ = ffx::FFX_API_QUERY_DESC_TYPE_UPSCALE_GETJITTERPHASECOUNT;
        phase_query.renderWidth = self.render_resolution.x;
        phase_query.displayWidth = self.display_resolution.x;
        phase_query.pOutPhaseCount = &mut phase_count;

        let _ = unsafe {
            ffx::query(
                &mut self.context,
                &mut phase_query.header as *mut _ as *mut ffx::ffxQueryDescHeader,
            )
        };

        // Query jitter offset.
        let mut jitter_x: f32 = 0.0;
        let mut jitter_y: f32 = 0.0;
        let mut jitter_query: ffx::ffxQueryDescUpscaleGetJitterOffset =
            unsafe { core::mem::zeroed() };
        jitter_query.header.type_ = ffx::FFX_API_QUERY_DESC_TYPE_UPSCALE_GETJITTEROFFSET;
        jitter_query.index = frame_index as i32;
        jitter_query.phaseCount = phase_count;
        jitter_query.pOutX = &mut jitter_x;
        jitter_query.pOutY = &mut jitter_y;

        let _ = unsafe {
            ffx::query(
                &mut self.context,
                &mut jitter_query.header as *mut _ as *mut ffx::ffxQueryDescHeader,
            )
        };

        Vec2::new(jitter_x, jitter_y)
    }

    /// Encode and execute FSR 3.1 upscale dispatch commands.
    ///
    /// This method:
    /// 1. Records resource transitions into `encoder`.
    /// 2. Submits `encoder` to `queue` so transitions are applied on the GPU
    ///    **before** FSR reads the textures.
    /// 3. Executes the FSR dispatch on the raw DX12 queue.
    /// 4. Replaces `encoder` with a fresh one for subsequent work.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render(
        &mut self,
        encoder: &mut CommandEncoder,
        queue: &wgpu::Queue,
        color: &TextureView,
        depth: &TextureView,
        motion_vectors: &TextureView,
        output: &TextureView,
        reset: bool,
        jitter: Vec2,
        render_size: UVec2,
        delta_time_ms: f32,
    ) -> Result<(), FsrError> {
        // Transition input textures to RESOURCE and output to STORAGE_READ_WRITE.
        encoder.transition_resources(
            core::iter::empty(),
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

        // Submit the current encoder (prior passes + transitions) so that
        // resource states are correct on the GPU before the FSR dispatch.
        let submitted = std::mem::replace(
            encoder,
            self.device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("Post-FSR"),
                }),
        );
        queue.submit(std::iter::once(submitted.finish()));

        unsafe {
            // Wait for the *previous* FSR dispatch to finish so the command
            // allocator can be safely reset and reused.
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

            let fsr_color = self.texture_resource(
                color,
                render_size,
                ffx::FfxApiResourceState_FFX_API_RESOURCE_STATE_COMPUTE_READ,
            )?;
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
            let fsr_output = self.texture_resource(
                output,
                self.display_resolution,
                ffx::FfxApiResourceState_FFX_API_RESOURCE_STATE_UNORDERED_ACCESS,
            )?;

            let mut desc: ffx::ffxDispatchDescUpscale = core::mem::zeroed();
            desc.header.type_ = ffx::FFX_API_DISPATCH_DESC_TYPE_UPSCALE;
            desc.commandList = cmd_list.as_raw() as *mut core::ffi::c_void;

            desc.color = fsr_color;
            desc.depth = fsr_depth;
            desc.motionVectors = fsr_motion;
            desc.output = fsr_output;

            desc.jitterOffset = ffx::FfxApiFloatCoords2D {
                x: jitter.x,
                y: jitter.y,
            };
            desc.motionVectorScale = ffx::FfxApiFloatCoords2D {
                x: -(render_size.x as f32),
                y: -(render_size.y as f32),
            };
            desc.renderSize = ffx::FfxApiDimensions2D {
                width: render_size.x,
                height: render_size.y,
            };
            desc.upscaleSize = ffx::FfxApiDimensions2D {
                width: self.display_resolution.x,
                height: self.display_resolution.y,
            };
            desc.frameTimeDelta = delta_time_ms;
            desc.preExposure = 1.0;
            desc.reset = reset;

            tracing::debug!(
                "FSR: dispatching (render={}x{}, upscale={}x{})",
                render_size.x,
                render_size.y,
                self.display_resolution.x,
                self.display_resolution.y
            );

            ffx::dispatch(
                &mut self.context,
                &desc.header as *const _ as *const ffx::ffxDispatchDescHeader,
            )
            .map_err(|e| {
                tracing::error!("FSR: ffxDispatch failed: {e:?}");
                FsrError::Dispatch
            })?;

            cmd_list.Close().map_err(|_| FsrError::Dispatch)?;

            tracing::debug!("FSR: dispatch complete");

            // Execute the FSR command list on the DX12 queue.
            // Because we already submitted the wgpu encoder above, the resource
            // transitions are guaranteed to have been applied first (same queue,
            // FIFO ordering).
            self.dx12_queue
                .ExecuteCommandLists(&[Some(cmd_list.cast().map_err(|_| FsrError::Dispatch)?)]);

            // Signal the fence so the next frame knows when this dispatch is done.
            self.fence_value += 1;
            self.dx12_queue
                .Signal(&self.fence, self.fence_value)
                .map_err(|_| FsrError::Dispatch)?;
        }

        Ok(())
    }

    /// Block the CPU until the GPU has finished the most recent FSR dispatch.
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

impl Drop for FsrContext {
    fn drop(&mut self) {
        // Wait for the GPU to finish any in-flight FSR dispatch before
        // destroying the context and its resources.
        let _ = unsafe { self.wait_for_gpu() };
        let _ = unsafe { ffx::destroy_context(&mut self.context) };
    }
}

// SAFETY: The FSR context is only accessed from the render thread via NonSend.
// The context internally synchronises through command buffer submission.
unsafe impl Send for FsrContext {}
unsafe impl Sync for FsrContext {}
