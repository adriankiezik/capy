use bevy_ecs::world::World;

use crate::resources::trace::TracePipeline;
use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::resources::{GpuContext, RenderResolution, RendererSettings, TraceStatsReporter};
use crate::shader_source::TraceShaderFeatures;

pub(crate) fn init_trace(world: &mut World) {
    let Some(scene) = world.get_non_send_resource::<VoxelSceneBuffers>() else {
        tracing::warn!("Missing VoxelSceneBuffers resource.");
        return;
    };

    let gpu = world.non_send_resource::<GpuContext>();
    let resolution = world.resource::<RenderResolution>();
    let features = world
        .get_resource::<RendererSettings>()
        .map(TraceShaderFeatures::from_settings)
        .unwrap_or_else(|| TraceShaderFeatures::from_settings(&RendererSettings::default()));

    let pipeline = TracePipeline::new(
        &gpu.device,
        resolution.width,
        resolution.height,
        scene,
        features,
    );

    world.insert_non_send_resource(pipeline);
    world.get_resource_or_init::<TraceStatsReporter>();
}
