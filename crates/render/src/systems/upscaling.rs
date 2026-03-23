use bevy_ecs::world::World;

use crate::resources::{
    DEFAULT_RENDER_SCALE, FsrPipeline, FsrSettings, GpuContext, RenderResolution, RendererSettings,
    TemporalCameraState, compute_scaled_resolution,
};

#[cfg(feature = "dlss")]
use crate::resources::{AoMode, DlssPipeline, DlssSettings};

pub(crate) fn init_upscaling(world: &mut World) {
    world.insert_resource(TemporalCameraState::default());
    #[cfg(feature = "dlss")]
    world.insert_non_send_resource(DlssPipeline::new());
    world.insert_non_send_resource(FsrPipeline::new());
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
    let mut render_resolution =
        compute_scaled_resolution(output_width, output_height, render_scale);
    let mut reset_temporal = false;

    // Track whether DLSS is active so FSR can be skipped when DLSS takes priority.
    #[cfg(feature = "dlss")]
    let mut dlss_active = false;

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
                        dlss_active = true;
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
                        dlss_active = true;
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

    {
        // Skip FSR when DLSS is active (DLSS takes priority).
        #[cfg(feature = "dlss")]
        let skip_fsr = dlss_active;
        #[cfg(not(feature = "dlss"))]
        let skip_fsr = false;

        let fsr_settings = world.get_resource::<FsrSettings>().cloned();
        let (device, adapter) = {
            let gpu = world.non_send_resource::<GpuContext>();
            (gpu.device.clone(), gpu.adapter.clone())
        };

        let mut fsr_supported = false;
        if let Some(mut fsr) = world.get_non_send_resource_mut::<FsrPipeline>() {
            let was_active = fsr.output_texture().is_some();

            if skip_fsr {
                if was_active {
                    fsr.deactivate();
                    reset_temporal = true;
                }
            } else {
                match fsr_settings.as_ref() {
                    Some(settings) if settings.enabled => {
                        if let Some((fsr_resolution, recreated)) = fsr.configure(
                            settings,
                            &device,
                            &adapter,
                            [output_width, output_height],
                        ) {
                            render_resolution = (fsr_resolution[0], fsr_resolution[1]);
                            reset_temporal |= recreated || settings.reset;
                            fsr_supported = true;
                        } else if was_active {
                            reset_temporal = true;
                        }
                    }
                    Some(_) if was_active => {
                        fsr.deactivate();
                        reset_temporal = true;
                    }
                    None if was_active => {
                        fsr.deactivate();
                        reset_temporal = true;
                    }
                    _ => {}
                }
            }
        }

        let is_vulkan = world
            .non_send_resource::<GpuContext>()
            .adapter
            .get_info()
            .backend
            == wgpu::Backend::Vulkan;
        if let Some(mut settings) = world.get_resource_mut::<FsrSettings>() {
            settings.supported = fsr_supported || is_vulkan;
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
