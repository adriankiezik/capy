#![allow(clippy::unwrap_used)]

use super::*;
use crate::world::{self, MaterialId, OwnerId, StructureId, VOXEL_SIZE, Voxel, WorldError};
use glam::{IVec3, Vec3};
use proptest::prelude::*;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use world::tests::voxel;

pub(crate) fn settings() -> SimulationConfig {
    SimulationConfig {
        world: world::tests::settings(),
        physics: PhysicsConfig::default(),
        max_bodies: 128,
    }
}

pub(crate) fn place(scene: &mut Simulation, voxels: impl IntoIterator<Item = (IVec3, Voxel)>) {
    let mut transaction = scene.transaction();

    for (p, value) in voxels {
        transaction.place(p, value).unwrap();
    }

    scene.commit(transaction).unwrap();
}

fn structure_settings() -> SimulationConfig {
    let mut config = settings();

    config.world.bounds.min.y = -8;

    for owner in &mut config.world.owners[1..] {
        owner.structure = StructureId(2);
    }

    config
}

fn bridge(config: SimulationConfig) -> Simulation {
    let mut scene = Simulation::new(config).unwrap();

    place(
        &mut scene,
        (-8..0)
            .flat_map(|y| [0, 4].map(|x| (IVec3::new(x, y, 0), voxel(1))))
            .chain([
                (IVec3::new(0, 0, 0), voxel(1)),
                (IVec3::new(4, 0, 0), voxel(1)),
                (IVec3::new(0, 1, 0), voxel(2)),
                (IVec3::new(1, 1, 0), voxel(3)),
                (IVec3::new(2, 1, 0), Voxel::new(MaterialId(2), OwnerId(2))),
                (IVec3::new(3, 1, 0), voxel(3)),
                (IVec3::new(4, 1, 0), voxel(2)),
            ]),
    );

    scene
}

fn body_voxels(scene: &Simulation) -> BTreeMap<[i32; 3], Voxel> {
    let mut occupied = BTreeMap::new();

    for body in &scene.bodies {
        for (p, value) in body.geometry.occupied() {
            let p = (p.as_vec3() + body.translation / VOXEL_SIZE)
                .round()
                .as_ivec3()
                .to_array();

            assert!(
                occupied.insert(p, value).is_none(),
                "duplicate detached voxel {p:?}"
            );
        }
    }

    occupied
}

#[test]
fn severed_structure_preserves_voxels_mass_and_snapshot_then_settles_and_wakes() {
    let mut scene = bridge(structure_settings());

    assert!(scene.bodies.is_empty());

    let original = scene.snapshot();

    let mut transaction = scene.transaction();

    transaction.remove(IVec3::new(0, 0, 0)).unwrap();

    scene.commit(transaction).unwrap();

    assert!(
        scene.bodies.is_empty(),
        "the other foundation still supports the connected bridge"
    );

    let mut transaction = scene.transaction();

    transaction.remove(IVec3::new(2, 1, 0)).unwrap();

    scene.commit(transaction).unwrap();

    assert_eq!(scene.bodies.len(), 1);

    assert_eq!(
        body_voxels(&scene),
        BTreeMap::from([([0, 1, 0], voxel(2)), ([1, 1, 0], voxel(3)),])
    );

    for x in 0..=4 {
        let p = IVec3::new(x, 1, 0);

        assert!(!original.voxel(p).unwrap().is_empty());

        assert_eq!(scene.world.voxel(p).unwrap().is_empty(), x <= 2);
    }

    let remaining_mass: f32 = (2..=3)
        .flat_map(|owner| scene.world.owner_voxels(OwnerId(owner)))
        .map(|(_, v)| scene.world.material(v.material).unwrap().density * VOXEL_SIZE.powi(3))
        .sum();

    assert!((remaining_mass + scene.stats().dynamic_mass - 4.0).abs() < 1e-5);

    for _ in 0..120 {
        scene.advance(Duration::from_secs_f32(1.0 / 60.0)).unwrap();

        let bottom = scene.bodies[0].translation.y + scene.bodies[0].geometry.min.y;

        assert!(bottom >= -0.8, "body penetrated the world floor");
    }

    let body = &scene.bodies[0];

    assert!(body.sleeping);

    assert_eq!(body.velocity, 0.0);

    assert!(
        (body.translation.y + body.geometry.min.y - scene.settings.physics.contact_slop).abs()
            < 1e-5
    );

    let mut transaction = scene.transaction();

    transaction.remove(IVec3::new(4, 0, 0)).unwrap();

    scene.commit(transaction).unwrap();

    assert_eq!(scene.bodies.len(), 2);

    assert!(scene.bodies.iter().all(|body| !body.sleeping));

    assert_ne!(scene.bodies[0].id, scene.bodies[1].id);

    assert!((scene.stats().dynamic_mass - 4.0).abs() < 1e-5);

    assert_eq!(original.voxel(IVec3::new(4, 0, 0)).unwrap(), voxel(1));
}

#[test]
fn touching_different_structures_detach_separately_and_stack_without_penetration() {
    let mut config = structure_settings();

    config.world.owners[2].structure = StructureId(3);

    let mut scene = Simulation::new(config).unwrap();

    place(
        &mut scene,
        (-8..0).map(|y| (IVec3::new(0, y, 0), voxel(1))).chain([
            (IVec3::ZERO, voxel(1)),
            (IVec3::Y, voxel(2)),
            (IVec3::Y * 2, voxel(3)),
        ]),
    );

    assert!(scene.bodies.is_empty());

    let mut transaction = scene.transaction();

    transaction.remove(IVec3::ZERO).unwrap();

    scene.commit(transaction).unwrap();

    assert_eq!(scene.bodies.len(), 2);

    for _ in 0..180 {
        scene.advance(Duration::from_secs_f32(1.0 / 60.0)).unwrap();

        let mut intervals: Vec<_> = scene
            .bodies
            .iter()
            .map(|b| {
                (
                    b.translation.y + b.geometry.min.y,
                    b.translation.y + b.geometry.max.y,
                )
            })
            .collect();

        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));

        assert!(intervals[0].0 >= -0.8 - 1e-6);

        assert!(intervals[0].1 <= intervals[1].0 + 1e-6, "{intervals:?}");
    }

    assert_eq!(scene.stats().sleeping_bodies, 2);
}

fn assert_unchanged(scene: &Simulation, world: &world::WorldRead, bodies: &[Body]) {
    assert!(Arc::ptr_eq(&scene.world.root, &world.root));

    assert_eq!(scene.bodies.len(), bodies.len());

    for (actual, expected) in scene.bodies.iter().zip(bodies) {
        assert_eq!(actual.id, expected.id);

        assert!(Arc::ptr_eq(&actual.geometry, &expected.geometry));

        assert_eq!(actual.translation, expected.translation);

        assert_eq!(actual.velocity, expected.velocity);

        assert_eq!(actual.sleeping, expected.sleeping);
    }
}

// Deliberately gives the engine too little room or work allowance to finish an edit or
// falling movement. A failed attempt must leave the world exactly as it was, and retrying
// with enough resources must work without missing blocks or skipped object identities.
#[test]
fn failed_commits_and_simulation_steps_are_atomic_and_recoverable() {
    for failure in ["support", "bodies", "storage", "collision"] {
        let mut scene = bridge(structure_settings());

        match failure {
            "support" => {
                Arc::make_mut(&mut Arc::make_mut(&mut scene.world.root).settings)
                    .max_support_voxels = 1
            }
            "bodies" => scene.settings.max_bodies = 1,
            "storage" => {
                Arc::make_mut(&mut Arc::make_mut(&mut scene.world.root).settings).max_leaves = 1
            }
            _ => {}
        }

        let world = scene.snapshot();

        let bodies = scene.bodies.clone();

        let mut transaction = scene.transaction();

        transaction.remove(IVec3::ZERO).unwrap();

        transaction.remove(IVec3::new(4, 0, 0)).unwrap();

        transaction.remove(IVec3::new(2, 1, 0)).unwrap();

        if failure == "storage" {
            transaction.place(IVec3::new(16, 0, 0), voxel(1)).unwrap();
        }

        if failure == "collision" {
            scene.commit(transaction).unwrap();

            let world = scene.snapshot();

            let bodies = scene.bodies.clone();

            scene.settings.physics.max_collision_probes = 1;

            assert!(matches!(
                scene.advance(Duration::from_millis(100)),
                Err(SimulationError::Limit(_))
            ));

            assert_unchanged(&scene, &world, &bodies);

            scene.settings.physics.max_collision_probes = 1_000_000;

            scene.advance(Duration::from_millis(100)).unwrap();

            assert!(scene.bodies.iter().all(|b| b.translation.y < 0.0));
        } else {
            let result = scene.commit(transaction);

            match failure {
                "support" => assert!(matches!(
                    result,
                    Err(SimulationError::Limit("support scan"))
                )),
                "bodies" => assert!(matches!(
                    result,
                    Err(SimulationError::Limit("dynamic bodies"))
                )),
                "storage" => assert!(matches!(
                    result,
                    Err(SimulationError::World(WorldError::Limit("storage leaves")))
                )),
                _ => unreachable!(),
            }

            assert_unchanged(&scene, &world, &bodies);

            Arc::make_mut(&mut Arc::make_mut(&mut scene.world.root).settings).max_support_voxels =
                16384;
            Arc::make_mut(&mut Arc::make_mut(&mut scene.world.root).settings).max_leaves = 4096;
            scene.settings.max_bodies = 128;

            let mut retry = scene.transaction();

            for p in [IVec3::ZERO, IVec3::new(4, 0, 0), IVec3::new(2, 1, 0)] {
                retry.remove(p).unwrap();
            }

            scene.commit(retry).unwrap();

            assert_eq!(
                scene.bodies.iter().map(|b| b.id).collect::<Vec<_>>(),
                vec![1, 2]
            );

            assert_eq!(body_voxels(&scene).len(), 4);
        }
    }
}

fn box_distance(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3, maximum: f32) -> Option<f32> {
    let mut near = 0.0f64;

    let mut far = maximum as f64;

    for axis in 0..3 {
        if direction[axis] == 0.0 {
            if origin[axis] < min[axis] || origin[axis] >= max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] as f64 - origin[axis] as f64) / direction[axis] as f64;

            let b = (max[axis] as f64 - origin[axis] as f64) / direction[axis] as f64;

            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }

    (near <= far).then_some(near as f32)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    // Aims in many directions through randomly arranged blocks. Checks that the engine
    // finds the same nearest block, at the same distance, as checking every block individually.
    #[test]
    fn raycasts_match_brute_force_voxel_boxes(
        coordinates in prop::collection::btree_set((-3i32..4, -3i32..4, -3i32..4), 1..40),
        origin in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
        direction in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
    ) {
        let origin = Vec3::new(origin.0, origin.1, origin.2);
        let direction = Vec3::new(direction.0, direction.1, direction.2);
        prop_assume!(direction.length_squared() > 0.01);
        let direction = direction.normalize();
        let mut scene = Simulation::new(settings()).unwrap();
        let coordinates: Vec<_> = coordinates.into_iter().map(|(x, y, z)| IVec3::new(x, y, z)).collect();
        place(&mut scene, coordinates.iter().map(|&p| (p, voxel(1))));
        let mut expected: Vec<_> = coordinates.iter().filter_map(|&p| {
            let min = p.as_vec3() * VOXEL_SIZE;
            box_distance(origin, direction, min, min + Vec3::splat(VOXEL_SIZE), 3.0).map(|distance| (p, distance))
        }).collect();
        expected.sort_by(|a, b| a.1.total_cmp(&b.1));
        if expected.len() > 1 {
            prop_assume!((expected[0].1 - expected[1].1).abs() > 1e-5);
        }
        let actual = scene.raycast(origin, direction, 3.0).unwrap();
        assert_eq!(actual.is_some(), !expected.is_empty());
        if let Some(hit) = actual {
            let coordinate = match hit.target {
                Target::Static(p) => p,
                Target::Dynamic { body, voxel } => {
                    let body = scene.bodies.iter().find(|b| b.id == body).unwrap();
                    (voxel.as_vec3() + body.translation / VOXEL_SIZE).round().as_ivec3()
                }
            };
            assert_eq!(coordinate, expected[0].0);
            assert_eq!(hit.voxel, voxel(1));
            assert!((hit.distance - expected[0].1).abs() < 1e-5);
        }
    }
}

// Lets a block fall, then checks that aiming and collision checks find it at its new
// position rather than its old one. A stationary block closer to the viewer must be
// selected before the falling block behind it.
#[test]
fn queries_track_translated_bodies_and_nearest_static_occluders() {
    let mut scene = Simulation::new(structure_settings()).unwrap();

    place(
        &mut scene,
        (-8..=-2).map(|y| (IVec3::new(1, y, 0), voxel(1))).chain([
            (IVec3::new(0, 10, 0), voxel(2)),
            (IVec3::new(0, -2, 0), voxel(1)),
        ]),
    );

    scene.advance(Duration::from_millis(200)).unwrap();

    let body = &scene.bodies[0];

    let min = body.translation + body.geometry.min;

    let max = body.translation + body.geometry.max;

    let center = (min + max) * 0.5;

    let hit = scene
        .raycast(center + Vec3::Y, -Vec3::Y, 3.0)
        .unwrap()
        .unwrap();

    assert_eq!(
        hit.target,
        Target::Dynamic {
            body: body.id,
            voxel: IVec3::new(0, 2, 0)
        }
    );

    assert_eq!(hit.normal, IVec3::Y);

    assert!((hit.distance - 0.95).abs() < 1e-5);

    assert!(
        scene
            .intersects(crate::Aabb {
                min: min + Vec3::splat(0.01),
                max: max - Vec3::splat(0.01)
            })
            .unwrap()
    );

    assert!(
        !scene
            .intersects(world::voxel_bounds(IVec3::new(0, 10, 0)))
            .unwrap()
    );

    let hit = scene
        .raycast(Vec3::new(0.05, -0.5, 0.05), Vec3::Y, 3.0)
        .unwrap()
        .unwrap();

    assert_eq!(hit.target, Target::Static(IVec3::new(0, -2, 0)));

    assert_eq!(hit.normal, -IVec3::Y);
}

// Aims straight at a block from all six directions, including from outside the world,
// and checks the hit side and distance. Starting inside the block should hit immediately;
// an aim that stops too short should miss. Invalid requests and exhausted search limits
// must report an error rather than return a misleading result.
#[test]
fn raycasts_handle_axis_parallel_entry_inside_hits_and_distance_limits() {
    let mut config = settings();

    config.world.bounds = world::VoxelBounds {
        min: IVec3::splat(-8),
        max: IVec3::splat(8),
    };

    let mut scene = Simulation::new(config).unwrap();

    let p = IVec3::splat(-1);

    place(&mut scene, [(p, voxel(1))]);

    let center = (p.as_vec3() + Vec3::splat(0.5)) * VOXEL_SIZE;

    for normal in [
        IVec3::X,
        -IVec3::X,
        IVec3::Y,
        -IVec3::Y,
        IVec3::Z,
        -IVec3::Z,
    ] {
        let origin = center + normal.as_vec3() * 2.0;

        let direction = -normal.as_vec3();

        let hit = scene
            .raycast(origin, direction * 7.0, 3.0)
            .unwrap()
            .unwrap();

        let target = Target::Dynamic {
            body: scene.bodies[0].id,
            voxel: IVec3::splat(7),
        };

        assert_eq!(hit.target, target, "normal {normal}");

        assert_eq!(hit.normal, normal);

        assert!((hit.distance - 1.95).abs() < 1e-5);

        assert!(scene.raycast(origin, direction, 1.94).unwrap().is_none());

        let inside = scene.raycast(center, direction, 0.01).unwrap().unwrap();

        assert_eq!(inside.target, target);

        assert_eq!(inside.distance, 0.0);

        assert_eq!(inside.normal, IVec3::ZERO);
    }

    scene.settings.physics.max_collision_probes = 1;

    assert!(matches!(
        scene.raycast(center + Vec3::Y * 2.0, -Vec3::Y, 3.0),
        Err(SimulationError::Limit("query steps"))
    ));

    for (origin, direction, distance) in [
        (center, Vec3::ZERO, 1.0),
        (Vec3::splat(f32::NAN), Vec3::Y, 1.0),
        (center, Vec3::Y, f32::INFINITY),
        (center, Vec3::Y, 0.0),
    ] {
        assert!(matches!(
            scene.raycast(origin, direction, distance),
            Err(SimulationError::Invalid)
        ));
    }
}
