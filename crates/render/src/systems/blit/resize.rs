use bevy_ecs::system::NonSendMut;
#[cfg(feature = "dlss")]
use bevy_ecs::system::{NonSend, Res};

use crate::gpu_texture::GpuTexture;
use crate::resources::{BlitPipeline, GpuContext, LightingPipeline};
#[cfg(feature = "dlss")]
use crate::resources::{DlssPipeline, DlssSettings};

/// Returns (source_texture, is_dlss, is_rr).
#[cfg(feature = "dlss")]
fn blit_source<'a>(
    lighting: &'a LightingPipeline,
    dlss: Option<&'a DlssPipeline>,
) -> (&'a GpuTexture, bool, bool) {
    if let Some(dlss) = dlss {
        // Prefer RR output over SR output
        if let Some(rr_output) = dlss.rr_output_texture() {
            return (rr_output, true, true);
        }
        if let Some(sr_output) = dlss.output_texture() {
            return (sr_output, true, false);
        }
    }
    (&lighting.output_color, false, false)
}

#[cfg(not(feature = "dlss"))]
fn blit_source(lighting: &LightingPipeline) -> (&GpuTexture, bool, bool) {
    (&lighting.output_color, false, false)
}

pub(crate) fn resize_blit_system(
    gpu: NonSendMut<GpuContext>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    blit: Option<NonSendMut<BlitPipeline>>,
    #[cfg(feature = "dlss")] dlss: Option<NonSend<DlssPipeline>>,
    #[cfg(feature = "dlss")] dlss_settings: Option<Res<DlssSettings>>,
) {
    let (Some(lighting), Some(mut blit)) = (lighting, blit) else {
        return;
    };
    // On the DLSS reset frame (context just recreated), fall back to the lighting
    // output to avoid a black flash.  DLSS still runs in render_passes to prime
    // its temporal history; the blit switches to its output next frame.
    #[cfg(feature = "dlss")]
    let dlss_resetting = dlss_settings.as_deref().is_some_and(|s| s.reset);
    #[cfg(feature = "dlss")]
    let (source_texture, source_is_dlss, source_is_rr) = if dlss_resetting {
        (&lighting.output_color, false, false)
    } else {
        blit_source(&lighting, dlss.as_deref())
    };
    #[cfg(not(feature = "dlss"))]
    let (source_texture, source_is_dlss, source_is_rr) = blit_source(&lighting);
    #[cfg(feature = "dlss")]
    let dlss_gen = dlss.as_deref().map_or(0, |d| d.generation());
    #[cfg(not(feature = "dlss"))]
    let dlss_gen = 0u32;
    let (source_width, source_height) = if source_is_dlss {
        (gpu.config.width, gpu.config.height)
    } else {
        (lighting.width, lighting.height)
    };
    let needs_rebind = blit.width != gpu.config.width
        || blit.height != gpu.config.height
        || blit.source_width != source_width
        || blit.source_height != source_height
        || blit.source_is_dlss != source_is_dlss
        || blit.source_is_rr != source_is_rr
        || blit.dlss_generation != dlss_gen;
    if needs_rebind {
        blit.rebuild_bind_group(&gpu.device, source_texture);
        blit.width = gpu.config.width;
        blit.height = gpu.config.height;
        blit.source_width = source_width;
        blit.source_height = source_height;
        blit.source_is_dlss = source_is_dlss;
        blit.source_is_rr = source_is_rr;
        blit.dlss_generation = dlss_gen;
    }
}
