#![allow(clippy::expect_used)]

use super::{Scene, SceneSettings, SimulationSettings, VisualSettings, geometry};
use crate::{
    Aabb,
    world::{
        Material, MaterialId, Owner, OwnerId, StructureId, Support, Transaction, VOXEL_SIZE, Voxel,
        VoxelBounds, WorldRead, WorldSettings,
    },
};
use divan::{Bencher, black_box, counter::ItemsCount};
use glam::{IVec3, Vec3};
use std::{sync::Arc, time::Duration};

fn world(edge: i32) -> WorldRead {
    WorldRead::new(WorldSettings {
        bounds: VoxelBounds {
            min: IVec3::splat(-1),
            max: IVec3::splat(edge + 1),
        },
        materials: vec![Material {
            id: MaterialId(1),
            color: [0.5; 3],
            density: 1.0,
        }],
        owners: Vec::new(),
        max_leaves: 4096,
        max_edit_voxels: 65536,
        max_support_voxels: 65536,
    })
    .expect("valid mesh fixture settings")
}

fn mesh(bencher: Bencher, edge: i32, checkerboard: bool) {
    let world = world(edge);

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

fn scene() -> Scene {
    Scene::new(SceneSettings {
        world: WorldSettings {
            bounds: VoxelBounds {
                min: IVec3::ZERO,
                max: IVec3::new(2048, 64, 64),
            },
            materials: vec![Material {
                id: MaterialId(1),
                color: [0.5; 3],
                density: 1.0,
            }],
            owners: vec![
                Owner {
                    id: OwnerId(1),
                    structure: StructureId(1),
                    support: Support::Fixed,
                },
                Owner {
                    id: OwnerId(2),
                    structure: StructureId(2),
                    support: Support::Contact(OwnerId(1)),
                },
            ],
            max_leaves: 4096,
            max_edit_voxels: 65536,
            max_support_voxels: 65536,
        },
        visuals: VisualSettings::default(),
        simulation: SimulationSettings::default(),
        render_leaf_edge: 8,
        max_mesh_vertices: 1_000_000,
        max_bodies: 128,
    })
    .expect("valid scene fixture settings")
}

fn place(scene: &mut Scene, positions: impl IntoIterator<Item = IVec3>, owner: u64) {
    let mut transaction = scene.transaction();

    for position in positions {
        transaction
            .place(position, Voxel::new(MaterialId(1), OwnerId(owner)))
            .expect("valid fixture placement");
    }

    scene.commit(transaction).expect("valid fixture commit");
}

fn query_scene(distance: i32) -> Scene {
    let mut scene = scene();

    place(
        &mut scene,
        (0..16).flat_map(|z| (0..16).map(move |y| IVec3::new(distance, y, z))),
        1,
    );

    scene
}

#[divan::bench(args = [8, 64, 512])]
fn raycast_hit(bencher: Bencher, distance: i32) {
    let scene = query_scene(distance);

    let origin = Vec3::splat(VOXEL_SIZE * 0.5);

    assert!(
        scene
            .raycast(origin, Vec3::X, 100.0)
            .expect("valid ray")
            .is_some()
    );

    bencher.bench_local(|| {
        black_box(&scene)
            .raycast(black_box(origin), Vec3::X, 100.0)
            .expect("valid ray")
    });
}

#[divan::bench(args = [8, 64, 512])]
fn raycast_miss(bencher: Bencher, distance: i32) {
    let scene = query_scene(distance);

    let origin = Vec3::new(0.05, 2.05, 0.05);

    let length = (distance + 1) as f32 * VOXEL_SIZE;

    assert!(
        scene
            .raycast(origin, Vec3::X, length)
            .expect("valid ray")
            .is_none()
    );

    bencher.bench_local(|| {
        black_box(&scene)
            .raycast(black_box(origin), Vec3::X, black_box(length))
            .expect("valid ray")
    });
}

fn intersection(bencher: Bencher, edge: i32, occupied: bool) {
    let scene = query_scene(32);

    let min = Vec3::new(if occupied { 3.2 } else { 0.1 }, 0.1, 0.1);

    let bounds = Aabb {
        min,
        max: min + Vec3::splat(edge as f32 * VOXEL_SIZE),
    };

    assert_eq!(scene.intersects(bounds).expect("valid bounds"), occupied);

    bencher.counter(ItemsCount::new(1_u64)).bench_local(|| {
        black_box(&scene)
            .intersects(black_box(bounds))
            .expect("valid bounds")
    });
}

#[divan::bench(args = [1, 8, 16])]
fn intersects_empty(bencher: Bencher, edge: i32) {
    intersection(bencher, edge, false);
}

#[divan::bench(args = [1, 8, 16])]
fn intersects_occupied(bencher: Bencher, edge: i32) {
    intersection(bencher, edge, true);
}

fn edit_fixture(count: i32, stride: i32) -> (Scene, Option<Transaction>) {
    let scene = scene();

    let mut transaction = scene.transaction();

    for x in 0..count {
        transaction
            .place(
                IVec3::new(x * stride, 0, 0),
                Voxel::new(MaterialId(1), OwnerId(1)),
            )
            .expect("valid edit fixture");
    }

    (scene, Some(transaction))
}

fn commit(bencher: Bencher, count: i32, stride: i32) {
    bencher
        .counter(ItemsCount::new(count as u64))
        .with_inputs(|| edit_fixture(count, stride))
        .bench_local_refs(|(scene, transaction)| {
            scene
                .commit(transaction.take().expect("fresh transaction"))
                .expect("valid benchmark commit");

            black_box(scene);
        });
}

#[divan::bench(args = [1, 16, 128])]
fn commit_contiguous(bencher: Bencher, count: i32) {
    commit(bencher, count, 1);
}

#[divan::bench(args = [1, 16, 128])]
fn commit_scattered(bencher: Bencher, count: i32) {
    commit(bencher, count, 8);
}

fn supported_beam(length: i32) -> (Scene, Option<Transaction>) {
    let mut scene = scene();

    place(&mut scene, [IVec3::ZERO], 1);

    place(&mut scene, (0..length).map(|x| IVec3::new(x, 1, 0)), 2);

    assert_eq!(scene.stats().bodies, 0);

    let mut transaction = scene.transaction();

    transaction.remove(IVec3::ZERO).expect("existing support");

    (scene, Some(transaction))
}

#[divan::bench(args = [8, 64, 512])]
fn commit_remove_support(bencher: Bencher, length: i32) {
    let (mut verification, transaction) = supported_beam(length);

    verification
        .commit(transaction.expect("support removal"))
        .expect("valid support removal");

    assert_eq!(verification.stats().bodies, 1);

    bencher
        .with_inputs(|| supported_beam(length))
        .bench_local_refs(|(scene, transaction)| {
            scene
                .commit(transaction.take().expect("fresh transaction"))
                .expect("valid support removal");

            black_box(scene);
        });
}

fn falling_scene(count: i32) -> Scene {
    let mut scene = scene();

    place(
        &mut scene,
        (0..count).map(|index| IVec3::new(index * 3, 8, 0)),
        2,
    );

    assert_eq!(scene.stats().bodies, count as usize);

    scene
}

fn advance(bencher: Bencher, count: i32, collision: bool, sleeping: bool) {
    let mut fixture = falling_scene(count);

    let delta = if collision {
        Duration::from_millis(100)
    } else {
        Duration::from_secs_f64(1.0 / 60.0)
    };

    if collision || sleeping {
        for body in &mut fixture.bodies {
            body.velocity = -8.0;
        }
    }

    if sleeping {
        fixture
            .advance(Duration::from_millis(100))
            .expect("settle fixture");

        assert_eq!(fixture.stats().sleeping_bodies, count as usize);
    }

    let bodies = fixture.bodies.clone();

    fixture.advance(delta).expect("valid verification step");

    assert_eq!(
        fixture.stats().sleeping_bodies,
        if collision || sleeping {
            count as usize
        } else {
            0
        }
    );

    bencher
        .counter(ItemsCount::new(count as u64))
        .with_inputs(|| {
            let mut scene = scene();

            scene.bodies = bodies.clone();

            scene
        })
        .bench_local_refs(|scene| {
            scene.advance(black_box(delta)).expect("valid physics step");

            black_box(&scene.bodies);
        });
}

#[divan::bench(args = [1, 16, 64])]
fn advance_falling(bencher: Bencher, count: i32) {
    advance(bencher, count, false, false);
}

#[divan::bench(args = [1, 16, 64])]
fn advance_floor_collision(bencher: Bencher, count: i32) {
    advance(bencher, count, true, false);
}

#[divan::bench(args = [1, 16, 64])]
fn advance_sleeping(bencher: Bencher, count: i32) {
    advance(bencher, count, false, true);
}

fn render_scene(leaves: i32) -> Scene {
    let mut scene = scene();

    place(
        &mut scene,
        (0..leaves).flat_map(|leaf| {
            (2..6).flat_map(move |z| {
                (2..6).flat_map(move |y| (2..6).map(move |x| IVec3::new(leaf * 8 + x, y, z)))
            })
        }),
        1,
    );

    assert_eq!(scene.meshes.len(), leaves as usize);

    scene
}

fn edited_render_scene(leaves: i32) -> Scene {
    let mut scene = render_scene(leaves);

    let meshes = scene.render_geometry().expect("warm mesh cache");

    assert_eq!(meshes.len(), leaves as usize);

    let sources = scene.meshes.clone();

    let mut transaction = scene.transaction();

    transaction
        .remove(IVec3::splat(3))
        .expect("existing interior voxel");

    scene.commit(transaction).expect("valid localized edit");

    assert_eq!(scene.meshes.len(), leaves as usize);

    assert_eq!(
        scene
            .meshes
            .iter()
            .filter(|(key, source)| {
                !Arc::ptr_eq(source, sources.get(*key).expect("existing mesh source"))
            })
            .count(),
        1
    );

    scene
}

#[divan::bench(args = [8, 64, 128])]
fn render_geometry_cold(bencher: Bencher, leaves: i32) {
    let verification = render_scene(leaves);

    assert_eq!(
        verification
            .render_geometry()
            .expect("valid cold geometry")
            .len(),
        leaves as usize
    );

    bencher
        .counter(ItemsCount::new(leaves as u64))
        .with_inputs(|| render_scene(leaves))
        .bench_local_refs(|scene| {
            black_box(&*scene)
                .render_geometry()
                .expect("valid cold geometry")
        });
}

#[divan::bench(args = [8, 64, 128])]
fn render_geometry_warm(bencher: Bencher, leaves: i32) {
    let scene = render_scene(leaves);

    let meshes = scene.render_geometry().expect("warm mesh cache");

    assert_eq!(meshes.len(), leaves as usize);

    let cached = scene.render_geometry().expect("read mesh cache");

    assert!(
        meshes
            .iter()
            .zip(&cached)
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

#[divan::bench(args = [8, 64, 128])]
fn render_geometry_local_edit(bencher: Bencher, leaves: i32) {
    let verification = edited_render_scene(leaves);

    assert_eq!(
        verification
            .render_geometry()
            .expect("valid edited geometry")
            .len(),
        leaves as usize
    );

    bencher
        .counter(ItemsCount::new(leaves as u64))
        .with_inputs(|| edited_render_scene(leaves))
        .bench_local_refs(|scene| {
            black_box(&*scene)
                .render_geometry()
                .expect("valid edited geometry")
        });
}
