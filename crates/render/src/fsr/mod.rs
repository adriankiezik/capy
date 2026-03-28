//! FSR 3.1 (FidelityFX Super Resolution) integration via custom FFI bindings
//! to the AMD FidelityFX SDK.
//!
//! Requires the `fsr` feature flag and a DX12 backend.

mod context;
pub(crate) mod fidelityfx;
pub(crate) mod frame_generation;

pub(crate) use context::FsrContext;

use thiserror::Error;

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
