use bevy_ecs::schedule::ScheduleLabel;

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Update;
