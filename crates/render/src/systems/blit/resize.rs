use bevy_ecs::system::{NonSend, NonSendMut, Res};

use crate::gpu_texture::GpuTexture;
use crate::resources::{BlitPipeline, FsrPipeline, FsrSettings, GpuContext, LightingPipeline};
#[cfg(feature = "dlss")]
use crate::resources::{DlssPipeline, DlssSettings};

/// Returns (source_texture, is_upscaled, is_rr).
fn blit_source<'a>(
    lighting: &'a LightingPipeline,
    #[cfg(feature = "dlss")] dlss: Option<&'a DlssPipeline>,
    fsr: Option<&'a FsrPipeline>,
) -> (&'a GpuTexture, bool, bool) {
    #[cfg(feature = "dlss")]
    if let Some(dlss) = dlss {
        // Prefer RR output over SR output
        if let Some(rr_output) = dlss.rr_output_texture() {
            return (rr_output, true, true);
        }
        if let Some(sr_output) = dlss.output_texture() {
            return (sr_output, true, false);
        }
    }
    if let Some(fsr) = fsr {
        if let Some(fsr_output) = fsr.output_texture() {
            return (fsr_output, true, false);
        }
    }
    (&lighting.output_color, false, false)
}

pub(crate) fn resize_blit_system(
    gpu: NonSendMut<GpuContext>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    blit: Option<NonSendMut<BlitPipeline>>,
    #[cfg(feature = "dlss")] dlss: Option<NonSend<DlssPipeline>>,
    #[cfg(feature = "dlss")] dlss_settings: Option<Res<DlssSettings>>,
    fsr: Option<NonSend<FsrPipeline>>,
    fsr_settings: Option<Res<FsrSettings>>,
) {
    let (Some(lighting), Some(mut blit)) = (lighting, blit) else {
        return;
    };
    // On an upscaler reset frame (context just recreated), fall back to the lighting
    // output to avoid a black flash. The upscaler still runs in render_passes to prime
    // its temporal history; the blit switches to its output next frame.
    #[cfg(feature = "dlss")]
    let dlss_resetting = dlss_settings.as_deref().is_some_and(|s| s.reset);
    let fsr_resetting = fsr_settings.as_deref().is_some_and(|s| s.reset);

    let upscaler_resetting = {
        let mut resetting = false;
        #[cfg(feature = "dlss")]
        {
            resetting |= dlss_resetting;
        }
        resetting |= fsr_resetting;
        resetting
    };

    let (source_texture, source_is_upscaled, source_is_rr) = if upscaler_resetting {
        (&lighting.output_color, false, false)
    } else {
        blit_source(
            &lighting,
            #[cfg(feature = "dlss")]
            dlss.as_deref(),
            fsr.as_deref(),
        )
    };

    let upscaler_gen = {
        let mut g = 0u32;
        #[cfg(feature = "dlss")]
        {
            g = g.wrapping_add(dlss.as_deref().map_or(0, |d| d.generation()));
        }
        g = g.wrapping_add(fsr.as_deref().map_or(0, |f| f.generation()));
        g
    };

    let (source_width, source_height) = if source_is_upscaled {
        (gpu.config.width, gpu.config.height)
    } else {
        (lighting.width, lighting.height)
    };
    let needs_rebind = blit.width != gpu.config.width
        || blit.height != gpu.config.height
        || blit.source_width != source_width
        || blit.source_height != source_height
        || blit.source_is_upscaled != source_is_upscaled
        || blit.source_is_rr != source_is_rr
        || blit.upscaler_generation != upscaler_gen;
    if needs_rebind {
        blit.rebuild_bind_group(&gpu.device, source_texture);
        blit.width = gpu.config.width;
        blit.height = gpu.config.height;
        blit.source_width = source_width;
        blit.source_height = source_height;
        blit.source_is_upscaled = source_is_upscaled;
        blit.source_is_rr = source_is_rr;
        blit.upscaler_generation = upscaler_gen;
    }
}
