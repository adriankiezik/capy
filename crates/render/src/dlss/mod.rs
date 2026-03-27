//! DLSS integration — patched fork of `dlss_wgpu` with raw Vulkan barrier fix.

mod feature_info;
mod initialization;
mod nvsdk_ngx;
mod sdk;

/// DLSS Frame Generation.
pub mod frame_generation;
/// DLSS Ray Reconstruction.
pub mod ray_reconstruction;
/// NVIDIA Reflex low-latency support.
pub(crate) mod reflex;
/// DLSS Super Resolution.
pub mod super_resolution;

pub use initialization::{
    FeatureSupport, InitializationError, create_instance, register_device_extensions,
    register_instance_extensions, request_device,
};
pub use nvsdk_ngx::{DlssError, DlssFeatureFlags, DlssPerfQualityMode};
pub use sdk::DlssSdk;
