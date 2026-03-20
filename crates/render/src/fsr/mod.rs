//! FSR 2 (FidelityFX Super Resolution) integration via the `fsr` crate.

mod context;

pub(crate) use context::FsrContext;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsrError {
    #[error("FSR error: {0}")]
    Fsr(#[from] fsr::Error),

    #[error("FSR requires a Vulkan adapter")]
    NotVulkan,

    #[error("failed to access Vulkan HAL from wgpu")]
    HalAccess,
}
