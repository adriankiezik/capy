use bevy_ecs::world::World;

use crate::gpu_texture::GpuTexture;
#[cfg(feature = "dlss")]
use crate::resources::DlssPipeline;
use crate::resources::{BlitPipeline, GpuContext, LightingPipeline};

/// Returns (source_texture, is_dlss, is_rr).
#[cfg(feature = "dlss")]
fn blit_source<'a>(
    lighting: &'a LightingPipeline,
    dlss: Option<&'a DlssPipeline>,
) -> (&'a GpuTexture, bool, bool) {
    if let Some(dlss) = dlss {
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

pub(crate) fn init_blit(world: &mut World) {
    let Some(lighting) = world.get_non_send_resource::<LightingPipeline>() else {
        tracing::warn!("Missing LightingPipeline resource.");
        return;
    };

    let gpu = world.non_send_resource::<GpuContext>();
    #[cfg(feature = "dlss")]
    let dlss = world.get_non_send_resource::<DlssPipeline>();

    #[cfg(feature = "dlss")]
    let (source_texture, source_is_dlss, source_is_rr) = blit_source(lighting, dlss);
    #[cfg(not(feature = "dlss"))]
    let (source_texture, source_is_dlss, source_is_rr) = blit_source(lighting);

    let mut pipeline = BlitPipeline::new(
        &gpu.device,
        source_texture,
        gpu.config.format,
        gpu.config.width,
        gpu.config.height,
        source_is_dlss,
    );
    pipeline.source_is_rr = source_is_rr;

    world.insert_non_send_resource(pipeline);
}
