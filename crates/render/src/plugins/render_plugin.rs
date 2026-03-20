use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
use bevy_ecs::world::World;

pub struct RenderPlugin;

impl capy_core::Plugin for RenderPlugin {
    fn register(&self, world: &mut World) {
        let mut schedules = world.get_resource_or_init::<Schedules>();

        #[cfg(feature = "dlss")]
        schedules.entry(capy_core::Startup).add_systems(
            (
                crate::systems::init_gpu,
                crate::systems::init_upscaling,
                crate::systems::voxel_scene::init_voxel_scene,
                crate::systems::trace::init_trace,
                crate::systems::gtao::init_gtao,
                crate::systems::rtao::init_rtao,
                crate::systems::lighting::init_lighting,
                crate::systems::blit::init_blit,
            )
                .chain(),
        );

        #[cfg(not(feature = "dlss"))]
        schedules.entry(capy_core::Startup).add_systems(
            (
                crate::systems::init_gpu,
                crate::systems::init_upscaling,
                crate::systems::voxel_scene::init_voxel_scene,
                crate::systems::trace::init_trace,
                crate::systems::gtao::init_gtao,
                crate::systems::lighting::init_lighting,
                crate::systems::blit::init_blit,
            )
                .chain(),
        );

        #[cfg(feature = "dlss")]
        schedules.entry(capy_core::Render).add_systems(
            (
                crate::systems::resize_surface_system,
                crate::systems::update_upscaling_system,
                crate::systems::voxel_scene::upload_uniforms_system,
                crate::systems::trace::resize_trace_system,
                crate::systems::gtao::resize_gtao_system,
                crate::systems::rtao::resize_rtao_system,
                crate::systems::lighting::resize_lighting_system,
                crate::systems::blit::resize_blit_system,
                crate::systems::run_compute_passes,
                crate::systems::render_passes_system,
                crate::systems::submit_frame_system,
            )
                .chain(),
        );

        #[cfg(not(feature = "dlss"))]
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
