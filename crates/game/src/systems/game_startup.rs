use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{Camera, CursorMode, GameWindow, MATERIAL_COLORS, VoxelMeshData};
use capy_shared::FlyCameraConfig;
use glam::Vec3;

const GRID_DIM_XZ: u32 = 32;
const HYBRID_MAX_QUADS_PER_CHUNK: usize = 1_000_000;

pub(crate) fn game_startup(world: &mut World) -> Result<(), BevyError> {
    if std::env::var("CAPY_STRESS_TEST").is_ok() {
        return stress_startup(world);
    }

    let world_dir = capy_assets::resolve_world_dir();
    let fs = capy_assets::OsFileSystem;
    let canonical = capy_world::generate_flat_baked()?;

    let mut edited = capy_assets::load_world_chunks(&world_dir, &fs).unwrap_or_default();
    for baked in edited.values_mut() {
        capy_world::recompute_foliage_bitmap(baked, capy_world::CHUNK_XZ);
    }
    let edited_meshes = capy_world::build_near_voxel_mesh_cache(
        &edited,
        capy_world::CHUNK_XZ,
        capy_world::CHUNK_Y,
        HYBRID_MAX_QUADS_PER_CHUNK,
    );
    let canonical_mesh = capy_world::build_near_voxel_chunk_mesh(
        &canonical,
        [0, 0, 0],
        capy_world::CHUNK_XZ,
        capy_world::CHUNK_Y,
        HYBRID_MAX_QUADS_PER_CHUNK,
    )
    .unwrap_or_default();
    let near_mesh = capy_world::assemble_hybrid_near_mesh(
        &edited_meshes,
        &canonical_mesh,
        &edited,
        GRID_DIM_XZ,
    );
    let mut mesh = if edited.is_empty() {
        VoxelMeshData::from_flat_world(
            &canonical,
            GRID_DIM_XZ,
            capy_world::CHUNK_XZ,
            capy_world::CHUNK_Y,
            MATERIAL_COLORS,
        )
    } else {
        VoxelMeshData::with_edited_chunks(
            &canonical,
            &edited,
            GRID_DIM_XZ,
            capy_world::CHUNK_XZ,
            capy_world::CHUNK_Y,
            MATERIAL_COLORS,
        )
    };
    tracing::info!(
        "[hybrid] meshed {}/{} edited chunks plus one canonical instance mesh",
        edited_meshes.len(),
        edited.len(),
    );
    mesh.set_near_mesh(near_mesh);

    let window = world.resource::<GameWindow>();
    let aspect = if window.height > 0 {
        window.width as f32 / window.height as f32
    } else {
        1.0
    };

    let camera = Camera {
        position: Vec3::new(128.0, 180.0, -20.0),
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: -0.2,
        aspect,
        ..Camera::default()
    };

    world.insert_resource(mesh);
    world.insert_resource(camera);
    world.insert_resource(CursorMode::Confined);
    world.insert_resource(FlyCameraConfig::default());

    Ok(())
}

fn stress_startup(world: &mut World) -> Result<(), BevyError> {
    tracing::info!("[stress] generating stress test world...");
    let t0 = std::time::Instant::now();

    let canonical = capy_world::generate_flat_baked()?;
    let stress_chunks = capy_world::generate_stress_world()?;
    let n = stress_chunks.len();

    let edited_meshes = capy_world::build_near_voxel_mesh_cache(
        &stress_chunks,
        capy_world::CHUNK_XZ,
        capy_world::CHUNK_Y,
        HYBRID_MAX_QUADS_PER_CHUNK,
    );
    let canonical_mesh = capy_world::build_near_voxel_chunk_mesh(
        &canonical,
        [0, 0, 0],
        capy_world::CHUNK_XZ,
        capy_world::CHUNK_Y,
        HYBRID_MAX_QUADS_PER_CHUNK,
    )
    .unwrap_or_default();
    let near_mesh = capy_world::assemble_hybrid_near_mesh(
        &edited_meshes,
        &canonical_mesh,
        &stress_chunks,
        GRID_DIM_XZ,
    );
    let mut mesh = VoxelMeshData::with_edited_chunks(
        &canonical,
        &stress_chunks,
        GRID_DIM_XZ,
        capy_world::CHUNK_XZ,
        capy_world::CHUNK_Y,
        MATERIAL_COLORS,
    );
    tracing::info!(
        "[hybrid] meshed {n_mesh}/{n} stress chunks: {} vertices, {} triangles",
        near_mesh.vertices.len(),
        near_mesh.indices.len() / 3,
        n_mesh = edited_meshes.len(),
    );
    mesh.set_near_mesh(near_mesh);

    let elapsed = t0.elapsed();
    tracing::info!("[stress] baked {n} chunks in {:.2}s", elapsed.as_secs_f64());

    let window = world.resource::<GameWindow>();
    let aspect = if window.height > 0 {
        window.width as f32 / window.height as f32
    } else {
        1.0
    };

    let camera = Camera {
        position: Vec3::new(0.0, 280.0, -400.0),
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: -0.3,
        aspect,
        ..Camera::default()
    };

    world.insert_resource(mesh);
    world.insert_resource(camera);
    world.insert_resource(CursorMode::Confined);
    world.insert_resource(FlyCameraConfig::default());

    Ok(())
}
