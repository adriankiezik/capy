use bevy_ecs::error::DefaultErrorHandler;
use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleLabel, Schedules};
use bevy_ecs::system::ScheduleSystem;
use bevy_ecs::world::World;
use capy_core::Plugin;
use winit::event_loop::{ControlFlow, EventLoop};

use crate::Result;
use crate::app::{App, capture_error};

type ScheduleBuilder = Box<dyn FnOnce(&mut World)>;

pub struct EngineBuilder {
    schedule_builders: Vec<ScheduleBuilder>,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self {
            schedule_builders: Vec::new(),
        }
    }

    pub fn add_plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        self.schedule_builders
            .push(Box::new(move |world: &mut World| {
                plugin.register(world);
            }));
        self
    }

    pub fn add_systems<M: 'static>(
        mut self,
        label: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M> + 'static,
    ) -> Self {
        self.schedule_builders
            .push(Box::new(move |world: &mut World| {
                world
                    .get_resource_or_init::<Schedules>()
                    .entry(label)
                    .add_systems(systems);
            }));
        self
    }

    pub fn run(self) -> Result<()> {
        let mut world = World::new();

        world.insert_resource(DefaultErrorHandler(capture_error));

        for builder in self.schedule_builders {
            builder(&mut world);
        }

        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);
        let mut app = App::new(world);
        event_loop.run_app(&mut app)?;
        if let Some(error) = app.error {
            return Err(error);
        }
        Ok(())
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
