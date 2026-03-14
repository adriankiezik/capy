use bevy_ecs::message::message_update_system;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
use bevy_ecs::world::World;

pub struct InputPlugin;

impl capy_core::Plugin for InputPlugin {
    fn register(&self, world: &mut World) {
        let mut schedules = world.get_resource_or_init::<Schedules>();
        schedules
            .entry(capy_core::PreStartup)
            .add_systems(crate::init_input_resources);
        schedules.entry(capy_core::Update).add_systems(
            (
                message_update_system,
                crate::apply_keyboard_messages,
                crate::apply_mouse_motion_messages,
                crate::flush_input_system,
                crate::sync_cursor_mode_system,
            )
                .chain(),
        );
    }
}
