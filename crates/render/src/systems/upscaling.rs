use bevy_ecs::world::World;

use crate::resources::{
    DEFAULT_RENDER_SCALE, GpuContext, RenderResolution, RendererSettings, TemporalCameraState,
    compute_scaled_resolution,
};

#[cfg(feature = "dlss")]
use crate::resources::{DlssPipeline, DlssSettings};

#[cfg(feature = "fsr")]
use crate::resources::{FsrPipeline, FsrSettings};

pub(crate) fn init_upscaling(world: &mut World) {
    world.insert_resource(TemporalCameraState::default());
    #[cfg(feature = "dlss")]
    world.insert_non_send_resource(DlssPipeline::new());
    #[cfg(feature = "fsr")]
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
    #[cfg(all(feature = "dlss", feature = "fsr"))]
    let mut dlss_active = false;

    #[cfg(feature = "dlss")]
    {
        let dlss_settings = world.get_resource::<DlssSettings>().cloned();
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
        let mut dlss_fg_active = false;
        if let Some(mut dlss) = world.get_non_send_resource_mut::<DlssPipeline>() {
            let was_sr_active = dlss.output_texture().is_some();
            let was_rr_active = dlss.rr_output_texture().is_some();
            match dlss_settings.as_ref() {
                Some(settings) => {
                    // Use DLSS super-resolution only; RR is intentionally disabled.
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
                        #[cfg(feature = "fsr")]
                        {
                            dlss_active = true;
                        }
                    } else if was_sr_active {
                        reset_temporal = true;
                    }

                    // FG configuration is deferred until after the DlssPipeline borrow is released.
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

        // Configure Reflex — enable or disable based on user setting.
        {
            let mut gpu = world.non_send_resource_mut::<GpuContext>();
            let reflex_enabled = dlss_settings.as_ref().is_some_and(|s| s.reflex_enabled);
            let sc = crate::dlss::reflex::raw_swapchain(&gpu.surface);
            if let (Some(reflex), Some(sc)) = (&mut gpu.reflex, sc) {
                if reflex_enabled && !reflex.is_enabled() {
                    reflex.enable(sc);
                } else if !reflex_enabled && reflex.is_enabled() {
                    reflex.disable(sc);
                }
            }
        }

        // Configure Frame Generation (deferred until DlssPipeline borrow is released).
        {
            let reflex_active = {
                let gpu = world.non_send_resource::<GpuContext>();
                gpu.reflex.as_ref().is_some_and(|r| r.is_enabled())
            };
            let dlss_settings_clone = world.get_resource::<DlssSettings>().cloned();
            if let (Some(mut dlss), Some(settings)) = (
                world.get_non_send_resource_mut::<DlssPipeline>(),
                dlss_settings_clone.as_ref(),
            ) {
                dlss_fg_active = dlss.configure_frame_generation(
                    settings,
                    &device,
                    &queue,
                    &adapter,
                    [output_width, output_height],
                    reflex_active,
                );
            }
        }

        // Configure swapchain for FG (latency + force Fifo so both presents display).
        {
            let mut gpu = world.non_send_resource_mut::<GpuContext>();
            gpu.set_frame_generation_mode(dlss_fg_active);
        }

        let fg_hw_supported = {
            let gpu = world.non_send_resource::<GpuContext>();
            gpu.dlss_fg_supported && dlss_supported
        };
        let reflex_hw_supported = {
            let gpu = world.non_send_resource::<GpuContext>();
            gpu.reflex.is_some()
        };

        if let Some(mut settings) = world.get_resource_mut::<DlssSettings>() {
            settings.supported = dlss_supported;
            settings.ray_reconstruction_supported = dlss_rr_supported;
            settings.frame_generation_supported = fg_hw_supported;
            settings.reflex_supported = reflex_hw_supported;
            if reset_temporal {
                settings.reset = true;
            }
        }
    }

    #[cfg(feature = "fsr")]
    {
        // Skip FSR when DLSS is active (DLSS takes priority).
        #[cfg(feature = "dlss")]
        let skip_fsr = dlss_active;
        #[cfg(not(feature = "dlss"))]
        let skip_fsr = false;

        let fsr_settings = world.get_resource::<FsrSettings>().cloned();
        let (device, queue, adapter) = {
            let gpu = world.non_send_resource::<GpuContext>();
            (gpu.device.clone(), gpu.queue.clone(), gpu.adapter.clone())
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
                            &queue,
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

        // Configure FSR Frame Generation (after upscaler configuration).
        if !skip_fsr {
            let mut fsr_fg_active = false;
            {
                let fsr_settings_clone = world.get_resource::<FsrSettings>().cloned();
                if let (Some(mut fsr), Some(settings)) = (
                    world.get_non_send_resource_mut::<FsrPipeline>(),
                    fsr_settings_clone.as_ref(),
                ) {
                    fsr_fg_active = fsr.configure_frame_generation(
                        settings,
                        &device,
                        &queue,
                        &adapter,
                        [output_width, output_height],
                    );
                }
            }

            // Configure swapchain for FG (latency + force Fifo so both presents display).
            // Only set from FSR when DLSS isn't already managing it.
            {
                let mut gpu = world.non_send_resource_mut::<GpuContext>();
                gpu.set_frame_generation_mode(fsr_fg_active);
            }
        }

        let is_dx12 = world.non_send_resource::<GpuContext>().backend == wgpu::Backend::Dx12;
        if let Some(mut settings) = world.get_resource_mut::<FsrSettings>() {
            settings.supported = fsr_supported || is_dx12;
            // FG is available on any DX12 GPU when the upscaler is active.
            settings.frame_generation_supported = is_dx12;
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
