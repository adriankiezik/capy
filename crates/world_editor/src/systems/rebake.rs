use std::sync::mpsc;
use std::time::Instant;

use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{MATERIAL_COLORS, VoxelMeshData};
use capy_render::GpuAccess;
use tracing::{error, info};

use crate::resources::{
    BakeTask, EditableWorld, MeshDirty, PendingEdits, PickPipeline, RebakeOutput, WorldGrid,
};

struct DirtyChunkSnapshot {
    delta_bricks: Vec<capy_world::LeafBrickEdit>,
    sparse_bricks: Vec<capy_world::LeafBrickEdit>,
}

pub(crate) fn rebake(world: &mut World) -> Result<(), BevyError> {
    let mut task = world.non_send_resource_mut::<BakeTask>();
    if let Some(rx) = task.pending.take() {
        match rx.try_recv() {
            Ok(Ok(result)) => {
                drop(task);
                apply_rebake_result(world, result);
            }
            Ok(Err(err)) => {
                drop(task);
                return Err(err.into());
            }
            Err(mpsc::TryRecvError::Empty) => {
                task.pending = Some(rx);
                return Ok(());
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                error!("[rebake] background worker disconnected before sending a result");
            }
        }
    } else {
        drop(task);
    }

    let mut pending = world.resource_mut::<PendingEdits>();
    let chunks_to_patch = std::mem::take(&mut pending.by_chunk);
    pending.full_rebuild = false;
    drop(pending);

    if chunks_to_patch.is_empty() {
        return Ok(());
    }

    let chunk_snapshots = {
        let editable = world.resource::<EditableWorld>();
        snapshot_dirty_chunks(&editable, chunks_to_patch)
    };

    {
        let mut dirty = world.resource_mut::<MeshDirty>();
        for coord in chunk_snapshots.keys() {
            dirty.dirty.remove(coord);
        }
    }

    let num_chunks = chunk_snapshots.len();
    let gpu = world.resource::<GpuAccess>().clone();
    let (canonical_baked, edited_baked, grid_dim_xz) = {
        let mut wg = world.resource_mut::<WorldGrid>();
        (
            wg.canonical_baked.clone(),
            std::mem::take(&mut wg.edited_baked),
            wg.grid_dim_xz,
        )
    };

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = build_rebake_output(
            canonical_baked,
            edited_baked,
            grid_dim_xz,
            chunk_snapshots,
            num_chunks,
            gpu,
        );
        let _ = tx.send(result);
    });

    world.non_send_resource_mut::<BakeTask>().pending = Some(rx);

    Ok(())
}

fn build_rebake_output(
    canonical_baked: capy_core::BakedChunkData,
    mut edited_baked: std::collections::HashMap<[i32; 3], capy_core::BakedChunkData>,
    grid_dim_xz: u32,
    chunks_to_patch: std::collections::HashMap<[i32; 3], DirtyChunkSnapshot>,
    num_chunks: usize,
    gpu: GpuAccess,
) -> anyhow::Result<RebakeOutput> {
    let t_patch = Instant::now();
    let mut total_bricks = 0usize;
    let mut jobs = Vec::with_capacity(num_chunks);

    for (coord, snapshot) in chunks_to_patch {
        total_bricks += snapshot.delta_bricks.len();

        if snapshot.sparse_bricks.is_empty() {
            edited_baked.remove(&coord);
            continue;
        }

        jobs.push((coord, snapshot.sparse_bricks));
    }

    let rebuilt_chunks = jobs.len();
    let patched_chunks: Vec<_> = if jobs.len() <= 1 {
        jobs.into_iter()
            .map(|(coord, mut bricks)| {
                let patched = capy_world::patch_baked_chunk_bricks_owned(
                    canonical_baked.clone(),
                    &mut bricks,
                );
                let compacted = capy_world::compact_baked_chunk(patched);
                (coord, compacted)
            })
            .collect()
    } else {
        std::thread::scope(|scope| {
            let handles: Vec<_> = jobs
                .into_iter()
                .map(|(coord, mut bricks)| {
                    let canonical = canonical_baked.clone();
                    scope.spawn(move || {
                        let patched =
                            capy_world::patch_baked_chunk_bricks_owned(canonical, &mut bricks);
                        let compacted = capy_world::compact_baked_chunk(patched);
                        (coord, compacted)
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|handle| match handle.join() {
                    Ok(result) => result,
                    Err(payload) => std::panic::resume_unwind(payload),
                })
                .collect()
        })
    };

    for (coord, patched) in patched_chunks {
        edited_baked.insert(coord, patched);
    }
    let patch_ms = t_patch.elapsed().as_secs_f64() * 1000.0;

    let t_mesh = Instant::now();
    let mesh = VoxelMeshData::with_edited_chunks(
        &canonical_baked,
        &edited_baked,
        grid_dim_xz,
        capy_world::CHUNK_XZ,
        capy_world::CHUNK_Y,
        MATERIAL_COLORS,
    );
    let mesh_ms = t_mesh.elapsed().as_secs_f64() * 1000.0;
    let upload = capy_render::prepare_voxel_scene_upload(&gpu, &mesh)?;

    Ok(RebakeOutput {
        edited_baked,
        mesh,
        upload,
        num_chunks,
        rebuilt_chunks,
        total_bricks,
        patch_ms,
        mesh_ms,
    })
}

fn apply_rebake_result(world: &mut World, result: RebakeOutput) {
    let RebakeOutput {
        edited_baked,
        mesh,
        upload,
        num_chunks,
        rebuilt_chunks,
        total_bricks,
        patch_ms,
        mesh_ms,
    } = result;

    let t_apply = Instant::now();
    let old_edited_baked = {
        let mut wg = world.resource_mut::<WorldGrid>();
        std::mem::replace(&mut wg.edited_baked, edited_baked)
    };

    let t_gpu = Instant::now();
    let buffers_recreated = capy_render::apply_prepared_voxel_scene_upload(world, &mesh, upload);
    let gpu_ms = t_gpu.elapsed().as_secs_f64() * 1000.0;

    let old_mesh = world.remove_resource::<VoxelMeshData>();
    world.insert_resource(mesh);

    if buffers_recreated {
        let device = world.resource::<GpuAccess>().device.clone();
        let voxels = world.resource::<capy_render::SharedVoxelBuffers>().clone();
        if let Some(mut pick) = world.get_non_send_resource_mut::<PickPipeline>() {
            pick.rebind(&device, &voxels);
        }
    }

    let apply_ms = t_apply.elapsed().as_secs_f64() * 1000.0;
    drop_async((old_edited_baked, old_mesh));

    info!(
        "[rebake] patch={patch_ms:.1}ms, mesh_rebuild={mesh_ms:.1}ms, \
         gpu_upload={gpu_ms:.1}ms, apply={apply_ms:.1}ms | chunks={num_chunks}, rebuilt={rebuilt_chunks}, \
         bricks={total_bricks}"
    );
}

fn drop_async<T>(value: T)
where
    T: Send + 'static,
{
    std::thread::spawn(move || drop(value));
}

fn snapshot_dirty_chunks(
    editable: &EditableWorld,
    chunks_to_patch: std::collections::HashMap<[i32; 3], Vec<capy_world::LeafBrickEdit>>,
) -> std::collections::HashMap<[i32; 3], DirtyChunkSnapshot> {
    let mut snapshots = std::collections::HashMap::with_capacity(chunks_to_patch.len());

    for (coord, delta_bricks) in chunks_to_patch {
        let sparse_bricks = editable
            .chunks
            .get(&coord)
            .map(|chunk| {
                chunk
                    .bricks
                    .iter()
                    .map(|(&brick_coord, &materials)| capy_world::LeafBrickEdit {
                        bx: brick_coord[0],
                        by: brick_coord[1],
                        bz: brick_coord[2],
                        materials,
                    })
                    .collect()
            })
            .unwrap_or_default();

        snapshots.insert(
            coord,
            DirtyChunkSnapshot {
                delta_bricks,
                sparse_bricks,
            },
        );
    }

    snapshots
}
