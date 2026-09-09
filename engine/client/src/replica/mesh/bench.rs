#![allow(clippy::expect_used)]
use super::geometry;
use crate::replica::world::{Leaf, Material, MaterialId, OwnerId, Voxel, VoxelBounds, WorldRead};
use divan::{Bencher, black_box, counter::ItemsCount};
use glam::IVec3;
use std::sync::Arc;

fn world(edge: i32) -> WorldRead {
    WorldRead {
        leaves: Default::default(),
        materials: Arc::new([(MaterialId(1), [0.5; 3])].into_iter().collect()),
        bounds: VoxelBounds {
            min: IVec3::splat(-1),
            max: IVec3::splat(edge + 1),
        },
    }
}

fn mesh(bencher: Bencher, edge: i32, checkerboard: bool) {
    let world = world(edge);

    let mut scratch = geometry::Scratch::default();

    let voxel = Voxel::new(MaterialId(1), OwnerId(1));

    let sample = |p: IVec3| {
        if p.cmpge(IVec3::ZERO).all()
            && p.cmplt(IVec3::splat(edge)).all()
            && (!checkerboard || (p.x + p.y + p.z) % 2 == 0)
        {
            voxel
        } else {
            Voxel::EMPTY
        }
    };

    bencher
        .counter(ItemsCount::new((edge as u64).pow(3)))
        .bench_local(|| {
            geometry::build(
                [0; 3],
                black_box(edge),
                sample,
                black_box(&world),
                1_000_000,
                true,
                &mut scratch,
            )
            .expect("mesh fixture fits vertex budget")
        });
}

#[divan::bench(args = [8, 16, 32])]
fn mesh_solid(bencher: Bencher, edge: i32) {
    mesh(bencher, edge, false);
}

#[divan::bench(args = [8, 16, 32])]
fn mesh_checkerboard(bencher: Bencher, edge: i32) {
    mesh(bencher, edge, true);
}

fn production_mesh(bencher: Bencher, edge: i32, reuse: bool) {
    let mut world = world(edge);

    for z in 0..edge / 8 {
        for y in 0..edge / 8 {
            for x in 0..edge / 8 {
                let coordinate = [x, y, z];

                world.leaves.insert(
                    coordinate,
                    Arc::new(Leaf(Chunk {
                        coordinate,
                        palette: vec![Voxel::new(MaterialId(1), OwnerId(1))],
                        indices: vec![0; 512],
                    })),
                );
            }
        }
    }

    let source = geometry::MeshSource::default();

    let resolve = |scratch: &mut geometry::Scratch| {
        source
            .resolve(
                [0; 3],
                black_box(edge),
                |key| world.leaves.get(&key).map(Arc::as_ref),
                black_box(&world),
                1_000_000,
                scratch,
            )
            .expect("valid production mesh")
    };

    let mut scratch = geometry::Scratch::default();

    assert_eq!(resolve(&mut scratch).vertices.len(), 36);

    assert!(source.ready().is_none());

    let bencher = bencher.counter(ItemsCount::new((edge as u64).pow(3)));

    if reuse {
        bencher.bench_local(|| resolve(&mut scratch));
    } else {
        drop(scratch);

        bencher
            .with_inputs(geometry::Scratch::default)
            .bench_local_refs(resolve);
    }
}

#[divan::bench(args = [8, 16, 32])]
fn mesh_production_fresh_scratch(bencher: Bencher, edge: i32) {
    production_mesh(bencher, edge, false);
}

#[divan::bench(args = [8, 16, 32])]
fn mesh_production_reused_scratch(bencher: Bencher, edge: i32) {
    production_mesh(bencher, edge, true);
}

use crate::replica::{Replica, ReplicaConfig};
use capy_engine_protocol::message::{Chunk, WorldDelta, WorldSnapshot};
use capy_engine_protocol::world::{Owner, StructureId};
use std::time::{Duration, Instant};

fn render_scene(count: i32) -> Replica {
    let chunks = (0..count)
        .map(|x| Chunk {
            coordinate: [x, 0, 0],
            palette: vec![Voxel::EMPTY, Voxel::new(MaterialId(1), OwnerId(1))],
            indices: (0..512)
                .map(|i| {
                    u16::from(
                        (2..6).contains(&(i % 8)) && i / 8 % 8 < 6 && (2..6).contains(&(i / 64)),
                    )
                })
                .collect(),
        })
        .collect();

    Replica::new(
        ReplicaConfig {
            render_leaf_edge: 8,
            ..Default::default()
        },
        0,
        WorldSnapshot {
            bounds: VoxelBounds {
                min: IVec3::ZERO,
                max: IVec3::new(2048, 64, 64),
            },
            materials: vec![Material {
                id: MaterialId(1),
                density: 1.0,
            }],
            owners: vec![Owner {
                id: OwnerId(1),
                structure: StructureId(1),
            }],
            chunks,
            bodies: Vec::new(),
        },
    )
    .expect("valid replica fixture")
}

fn complete(scene: &Replica) -> crate::replica::Result<Arc<[crate::replica::MeshInstance]>> {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let meshes = scene.render_geometry()?;

        if scene.meshes.values().all(|s| s.ready().is_some()) {
            return scene.render_geometry();
        }

        assert!(Instant::now() < deadline);

        drop(meshes);

        std::thread::yield_now();
    }
}

fn edited_render_scene(count: i32) -> Replica {
    let mut scene = render_scene(count);

    complete(&scene).expect("warm mesh cache");

    let sources = scene.meshes.clone();

    let mut chunk = scene.world.leaves[&[0, 0, 0]].0.clone();

    chunk.indices[3 + 8 * (3 + 8 * 3)] = 0;

    scene
        .apply(
            0,
            1,
            WorldDelta {
                chunks: vec![chunk],
                removed_chunks: Vec::new(),
                bodies: Vec::new(),
                removed_bodies: Vec::new(),
            },
        )
        .expect("valid local edit");

    assert_eq!(
        scene
            .meshes
            .iter()
            .filter(|(key, source)| !Arc::ptr_eq(source, &sources[*key]))
            .count(),
        2
    );

    scene
}

#[divan::bench(args = [8, 64, 128], ignore)]
fn mesh_cache_cold(bencher: Bencher, leaves: i32) {
    let verification = render_scene(leaves);

    assert_eq!(
        complete(&verification).expect("valid cold geometry").len(),
        leaves as usize
    );

    bencher
        .counter(ItemsCount::new(leaves as u64))
        .with_inputs(|| render_scene(leaves))
        .bench_local_refs(|scene| complete(black_box(&*scene)).expect("valid cold geometry"));
}

#[divan::bench(args = [8, 64, 128])]
fn mesh_cache_warm(bencher: Bencher, leaves: i32) {
    let scene = render_scene(leaves);

    let meshes = complete(&scene).expect("warm mesh cache");

    assert_eq!(meshes.len(), leaves as usize);

    let cached = complete(&scene).expect("read mesh cache");

    assert!(
        meshes
            .iter()
            .zip(cached.iter())
            .all(|(a, b)| Arc::ptr_eq(&a.mesh, &b.mesh))
    );

    drop(cached);

    drop(meshes);

    bencher
        .counter(ItemsCount::new(leaves as u64))
        .bench_local(|| {
            black_box(&scene)
                .render_geometry()
                .expect("valid cached geometry")
        });
}

#[divan::bench(args = [8, 64, 128], ignore)]
fn mesh_cache_local_edit(bencher: Bencher, leaves: i32) {
    let verification = edited_render_scene(leaves);

    assert_eq!(
        complete(&verification)
            .expect("valid edited geometry")
            .len(),
        leaves as usize
    );

    bencher
        .counter(ItemsCount::new(leaves as u64))
        .with_inputs(|| edited_render_scene(leaves))
        .bench_local_refs(|scene| complete(black_box(&*scene)).expect("valid edited geometry"));
}
