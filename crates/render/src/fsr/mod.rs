//! FSR 3.1 (FidelityFX Super Resolution) integration via custom FFI bindings
//! to the AMD FidelityFX SDK.
//!
//! Requires the `fsr` feature flag and a DX12 backend.

mod context;
pub(crate) mod fidelityfx;
pub(crate) mod frame_generation;

pub(crate) use context::FsrContext;
pub(crate) use frame_generation::{FsrFgCameraParams, FsrFrameGeneration};

use thiserror::Error;
use wgpu::TextureFormat;

/// Map a wgpu `TextureFormat` to the corresponding `FfxApiSurfaceFormat` value.
///
/// The FFI constants are `c_int` but `FfxApiResourceDescription::format` is `u32`,
/// so we cast here.
pub(crate) fn wgpu_to_ffx_format(fmt: TextureFormat) -> u32 {
    use fidelityfx::*;
    (match fmt {
        TextureFormat::Rgba8Unorm => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R8G8B8A8_UNORM,
        TextureFormat::Rgba8UnormSrgb => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R8G8B8A8_SRGB,
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
            // FSR has no BGRA constants — use RGBA; the DX12 resource carries the real format.
            FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R8G8B8A8_UNORM
        }
        TextureFormat::Rgba16Float => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R16G16B16A16_FLOAT,
        TextureFormat::Rg16Float => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R16G16_FLOAT,
        TextureFormat::R16Float => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R16_FLOAT,
        TextureFormat::Rgba32Float => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R32G32B32A32_FLOAT,
        TextureFormat::Rg32Float => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R32G32_FLOAT,
        TextureFormat::R32Float => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R32_FLOAT,
        TextureFormat::R8Unorm => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R8_UNORM,
        TextureFormat::Rg8Unorm => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R8G8_UNORM,
        TextureFormat::R16Unorm => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R16_UNORM,
        TextureFormat::R32Uint => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R32_UINT,
        TextureFormat::Depth32Float => FfxApiSurfaceFormat_FFX_API_SURFACE_FORMAT_R32_FLOAT,
        // Fallback — let the SDK infer from the DX12 resource.
        _ => 0,
    }) as u32
}

#[derive(Debug, Error)]
pub enum FsrError {
    #[error("FidelityFX SDK error: return code {0:#x}")]
    Sdk(u32),

    #[error("FSR requires a DirectX 12 adapter")]
    NotDx12,

    #[error("failed to access DX12 HAL from wgpu")]
    HalAccess,

    #[error("failed to create FSR context")]
    ContextCreation,

    #[error("FSR dispatch failed")]
    Dispatch,
}

impl FsrError {
    /// Convert an `ffxReturnCode_t` into a Result.
    pub(crate) fn from_ffx(code: u32) -> Result<(), Self> {
        // FFX_API_RETURN_OK == 0
        if code == 0 {
            Ok(())
        } else {
            Err(FsrError::Sdk(code))
        }
    }
}
