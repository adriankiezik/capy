use bevy_ecs::world::World;

use crate::gpu_texture::GpuTexture;
#[cfg(feature = "dlss")]
use crate::resources::DlssPipeline;
use crate::resources::{BlitPipeline, FsrPipeline, GpuContext, LightingPipeline};

/// Returns (source_texture, is_upscaled, is_rr).
fn blit_source<'a>(
    lighting: &'a LightingPipeline,
    #[cfg(feature = "dlss")] dlss: Option<&'a DlssPipeline>,
    fsr: Option<&'a FsrPipeline>,
) -> (&'a GpuTexture, bool, bool) {
    #[cfg(feature = "dlss")]
    if let Some(dlss) = dlss {
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

pub(crate) fn init_blit(world: &mut World) {
    let Some(lighting) = world.get_non_send_resource::<LightingPipeline>() else {
        tracing::warn!("Missing LightingPipeline resource.");
        return;
    };

    let gpu = world.non_send_resource::<GpuContext>();
    #[cfg(feature = "dlss")]
    let dlss = world.get_non_send_resource::<DlssPipeline>();
    let fsr = world.get_non_send_resource::<FsrPipeline>();

    let (source_texture, source_is_upscaled, source_is_rr) = blit_source(
        lighting,
        #[cfg(feature = "dlss")]
        dlss,
        fsr,
    );

    let mut pipeline = BlitPipeline::new(
        &gpu.device,
        source_texture,
        gpu.config.format,
        gpu.config.width,
        gpu.config.height,
        source_is_upscaled,
    );
    pipeline.source_is_rr = source_is_rr;

    world.insert_non_send_resource(pipeline);
}
