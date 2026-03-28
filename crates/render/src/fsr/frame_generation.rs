//! FSR 3 Frame Generation — stub for future integration.
//!
//! The FidelityFX SDK exposes frame generation through `ffxFrameGeneration*`
//! descriptors. This module reserves the type and API surface so it can be
//! fleshed out without restructuring the crate.

use super::FsrError;
use super::fidelityfx as ffx;

/// Wraps the FidelityFX frame-generation context.
pub(crate) struct FsrFrameGeneration {
    _context: ffx::ffxContext,
}

impl FsrFrameGeneration {
    /// Placeholder — full implementation will mirror DLSS FG:
    /// create context, evaluate, double-present.
    pub(crate) fn _new() -> Result<Self, FsrError> {
        Err(FsrError::ContextCreation)
    }
}

impl Drop for FsrFrameGeneration {
    fn drop(&mut self) {
        let _ = unsafe { ffx::destroy_context(&mut self._context) };
    }
}

// SAFETY: Same as FsrContext — render-thread only via NonSend.
unsafe impl Send for FsrFrameGeneration {}
unsafe impl Sync for FsrFrameGeneration {}
