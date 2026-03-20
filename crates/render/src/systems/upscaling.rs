use bevy_ecs::world::World;

use crate::resources::{
    DEFAULT_RENDER_SCALE, GpuContext, RenderResolution, RendererSettings, TemporalCameraState,
    compute_scaled_resolution,
};

#[cfg(feature = "dlss")]
use crate::resources::{AoMode, DlssPipeline, DlssSettings};

pub(crate) fn init_upscaling(world: &mut World) {
    world.insert_resource(TemporalCameraState::default());
    #[cfg(feature = "dlss")]
    world.insert_non_send_resource(DlssPipeline::new());
    update_upscaling_system(world);
}

pub(crate) fn update_upscaling_system(world: &mut World) {
    let (output_width, output_height) = {
        let gpu = world.non_send_resource::<GpuContext>();
        (gpu.config.width, gpu.config.height)
    };

    let render_scale = world
        .get_resource::<RendererSettings>()
        .map_or(DEFAULT_RENDER_SCALE, |settings| settings.render_scale);
    #[cfg(feature = "dlss")]
    let mut render_resolution =
        compute_scaled_resolution(output_width, output_height, render_scale);
    #[cfg(not(feature = "dlss"))]
    let render_resolution = compute_scaled_resolution(output_width, output_height, render_scale);
    #[cfg(feature = "dlss")]
    let mut reset_temporal = false;
    #[cfg(not(feature = "dlss"))]
    let reset_temporal = false;

    #[cfg(feature = "dlss")]
    {
        let dlss_settings = world.get_resource::<DlssSettings>().cloned();
        let (ao_mode, ao_enabled) = world
            .get_resource::<RendererSettings>()
            .map_or((AoMode::ScreenSpace, false), |s| {
                (s.ao_mode, s.ao_intensity > 0.0)
            });
        let (device, queue, adapter, dlss_extensions_enabled, rr_hw_supported) = {
            let gpu = world.non_send_resource::<GpuContext>();
            (
                gpu.device.clone(),
                gpu.queue.clone(),
                gpu.adapter.clone(),
                gpu.dlss_extensions_enabled,
                gpu.dlss_rr_supported,
            )
        };

        let mut dlss_supported = false;
        let mut dlss_rr_supported = false;
        if let Some(mut dlss) = world.get_non_send_resource_mut::<DlssPipeline>() {
            let was_sr_active = dlss.output_texture().is_some();
            let was_rr_active = dlss.rr_output_texture().is_some();
            match dlss_settings.as_ref() {
                Some(settings)
                    if settings.enabled
                        && ao_mode == AoMode::RayTraced
                        && ao_enabled
                        && rr_hw_supported =>
                {
                    // Deactivate SR if it was active (RR replaces SR)
                    if was_sr_active {
                        // SR will be deactivated inside configure_ray_reconstruction
                    }
                    if let Some((rr_resolution, recreated)) = dlss.configure_ray_reconstruction(
                        settings,
                        &device,
                        &queue,
                        [output_width, output_height],
                    ) {
                        render_resolution = (rr_resolution[0], rr_resolution[1]);
                        reset_temporal |= recreated || settings.reset;
                    } else if was_rr_active || was_sr_active {
                        reset_temporal = true;
                    }
                }
                Some(settings) => {
                    // Normal SR path — deactivate RR if it was active
                    if was_rr_active {
                        dlss.deactivate_ray_reconstruction();
                        reset_temporal = true;
                    }
                    if let Some((dlss_resolution, recreated)) = dlss.configure(
                        settings,
                        &device,
                        &queue,
                        &adapter,
                        dlss_extensions_enabled,
                        [output_width, output_height],
                    ) {
                        render_resolution = (dlss_resolution[0], dlss_resolution[1]);
                        reset_temporal |= recreated || settings.reset;
                    } else if was_sr_active {
                        reset_temporal = true;
                    }
                }
                None if was_sr_active || was_rr_active => {
                    dlss.deactivate();
                    reset_temporal = true;
                }
                None => {}
            }
            dlss_supported = dlss.is_supported();
            dlss_rr_supported = rr_hw_supported && dlss_supported;
        }

        if let Some(mut settings) = world.get_resource_mut::<DlssSettings>() {
            settings.supported = dlss_supported;
            settings.ray_reconstruction_supported = dlss_rr_supported;
            if reset_temporal {
                settings.reset = true;
            }
        }
    }

    if let Some(mut resolution) = world.get_resource_mut::<RenderResolution>() {
        resolution.width = render_resolution.0;
        resolution.height = render_resolution.1;
    } else {
        world.insert_resource(RenderResolution::new(
            render_resolution.0,
            render_resolution.1,
        ));
    }

    if reset_temporal {
        world.resource_mut::<TemporalCameraState>().reset_history();
    }
}
