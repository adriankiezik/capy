use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
use bevy_ecs::world::World;

pub struct RenderPlugin;

impl capy_core::Plugin for RenderPlugin {
    fn register(&self, world: &mut World) {
        // Ensure vendor DLLs in the `lib/` subdirectory are discoverable
        // before any GPU init system triggers a delay-loaded import.
        #[cfg(all(target_os = "windows", any(feature = "fsr", feature = "dlss")))]
        crate::add_lib_dll_search_path();

        let mut schedules = world.get_resource_or_init::<Schedules>();

        schedules.entry(capy_core::Startup).add_systems(
            (
                crate::systems::init_gpu,
                crate::systems::init_upscaling,
                |world: &mut World| {
                    tracing::debug!(">>> init_voxel_scene");
                    crate::systems::voxel_scene::init_voxel_scene(world);
                    tracing::debug!("<<< init_voxel_scene done");
                },
                |world: &mut World| {
                    tracing::debug!(">>> init_trace");
                    crate::systems::trace::init_trace(world);
                    tracing::debug!("<<< init_trace done");
                },
                |world: &mut World| {
                    tracing::debug!(">>> init_gtao");
                    crate::systems::gtao::init_gtao(world);
                    tracing::debug!("<<< init_gtao done");
                },
                |world: &mut World| {
                    tracing::debug!(">>> init_lighting");
                    crate::systems::lighting::init_lighting(world);
                    tracing::debug!("<<< init_lighting done");
                },
                |world: &mut World| {
                    tracing::debug!(">>> init_blit");
                    crate::systems::blit::init_blit(world);
                    tracing::debug!("<<< init_blit done");
                },
            )
                .chain(),
        );

        schedules.entry(capy_core::Render).add_systems(
            (
                crate::systems::resize_surface_system,
                crate::systems::update_upscaling_system,
                crate::systems::voxel_scene::upload_uniforms_system,
                crate::systems::trace::resize_trace_system,
                crate::systems::gtao::resize_gtao_system,
                crate::systems::lighting::resize_lighting_system,
                crate::systems::blit::resize_blit_system,
                crate::systems::run_compute_passes,
                crate::systems::render_passes_system,
                crate::systems::submit_frame_system,
            )
                .chain(),
        );
    }
}
