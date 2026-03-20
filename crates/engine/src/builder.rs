use bevy_ecs::error::DefaultErrorHandler;
use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleLabel, Schedules};
use bevy_ecs::system::ScheduleSystem;
use bevy_ecs::world::World;
use capy_core::Plugin;

use crate::Result;
use crate::error::EngineError;
use crate::headless::run_headless;
use crate::resources::Runner;
use crate::schedule_runner::capture_error;

type ScheduleBuilder = Box<dyn FnOnce(&mut World)>;

pub struct EngineBuilder {
    plugins: Vec<Box<dyn Plugin>>,
    schedule_builders: Vec<ScheduleBuilder>,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            schedule_builders: Vec::new(),
        }
    }

    pub fn add_plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        self.plugins.push(Box::new(plugin));
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
        use tracing_subscriber::EnvFilter;

        let filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("capy=info"));

        tracing_subscriber::fmt().with_env_filter(filter).init();

        let mut world = World::new();

        world.insert_resource(DefaultErrorHandler(capture_error));
        world.insert_resource(capy_core::FrameProfiler::new());

        let Self {
            plugins,
            schedule_builders,
        } = self;

        for plugin in &plugins {
            plugin.register(&mut world);
        }

        for builder in schedule_builders {
            builder(&mut world);
        }

        let runner = world
            .remove_resource::<Runner>()
            .unwrap_or_else(|| Runner(Box::new(run_headless)));

        (runner.0)(world).map_err(EngineError::Runner)
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
