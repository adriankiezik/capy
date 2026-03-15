use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
use bevy_ecs::world::World;

pub struct RenderPlugin;

impl capy_core::Plugin for RenderPlugin {
    fn register(&self, world: &mut World) {
        let mut schedules = world.get_resource_or_init::<Schedules>();

        schedules.entry(capy_core::Startup).add_systems(
            (
                crate::systems::init_gpu,
                crate::systems::streaming::init_streaming,
                crate::systems::blit::init_blit,
            )
                .chain(),
        );

        schedules.entry(capy_core::Render).add_systems(
            (
                crate::systems::streaming::upload_camera_system,
                crate::systems::resize_system,
                crate::systems::render_passes_system,
                crate::systems::submit_frame_system,
            )
                .chain(),
        );
    }
}
