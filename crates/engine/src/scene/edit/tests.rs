#![allow(clippy::unwrap_used)]

use super::*;
use crate::{
    scene::{
        connectivity::Connectivity,
        dynamics,
        tests::{place, settings},
    },
    world::tests::voxel,
};
use glam::{IVec3, Vec3};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn dynamic_scene(length: i32, protrusion: bool) -> Scene {
    let mut config = settings();

    config.world.bounds.min.y = 0;

    let mut scene = Scene::new(config).unwrap();

    let cells = (0..length).flat_map(|x| {
        (16..20).flat_map(move |y| (0..4).map(move |z| (IVec3::new(x, y, z), voxel(1))))
    });

    place(
        &mut scene,
        cells.chain(protrusion.then_some((IVec3::new(3, 20, 1), voxel(1)))),
    );

    assert_eq!(scene.bodies.len(), 1);

    scene
}

fn finish(scene: &mut Scene, edits: &mut SceneEdits) -> Vec<EditOutcome> {
    let deadline = Instant::now() + Duration::from_secs(5);

    let mut outcomes = Vec::new();

    while edits.pending() != 0 {
        outcomes.extend(edits.update(scene).unwrap());

        assert!(Instant::now() < deadline);

        std::thread::yield_now();
    }

    outcomes
}

// Starts removing a block from a moving object, then changes the ground far away.
// Checks that the unrelated ground change does not cancel the requested removal.
#[test]
fn unrelated_commit_preserves_in_flight_dynamic_edit() {
    let mut scene = dynamic_scene(16, false);

    let target = Target::Dynamic {
        body: scene.bodies[0].id,
        voxel: IVec3::new(3, 3, 1),
    };

    let mut edits = SceneEdits::new(&scene, EditSettings::default()).unwrap();

    assert!(edits.queue_remove(target));

    assert!(edits.update(&mut scene).unwrap().is_empty());

    place(&mut scene, [(IVec3::new(128, 0, 0), voxel(1))]);

    let outcomes = finish(&mut scene, &mut edits);

    assert_eq!(outcomes.len(), 1);

    assert!(outcomes[0].result.is_ok());

    assert!(
        scene.bodies[0]
            .geometry
            .voxel(IVec3::new(3, 3, 1))
            .is_empty()
    );
}

// Removes a protruding block from an object that is too complicated to draw within
// the drawing limit. Checks that the simpler result is allowed, and that trying
// to remove an already-empty space does not waste time preparing a new picture.
#[test]
fn local_removal_checks_resulting_budget_and_empty_removal_skips_meshing() {
    let mut scene = dynamic_scene(16, true);

    let id = scene.bodies[0].id;

    let target = IVec3::new(3, 4, 1);

    let mut reduced = scene.clone();

    reduced
        .remove(Target::Dynamic {
            body: id,
            voxel: target,
        })
        .unwrap();

    let vertices: usize = reduced
        .render_geometry()
        .unwrap()
        .iter()
        .map(|mesh| mesh.mesh.vertices.len())
        .sum();

    scene.settings.max_mesh_vertices = vertices;

    assert!(matches!(scene.render_geometry(), Err(SceneError::Limit(_))));

    let mut cache = Connectivity::default();

    let mut patch = pollster::block_on(
        Input::capture(&scene, Domain::Body(id))
            .unwrap()
            .prepare_local(vec![target], &mut cache),
    )
    .unwrap();

    assert!(matches!(patch, Patch::Local(_)));

    patch.publish(&mut scene).unwrap();

    assert!(scene.bodies[0].geometry.voxel(target).is_empty());

    assert_eq!(
        scene
            .render_geometry()
            .unwrap()
            .iter()
            .map(|mesh| mesh.mesh.vertices.len())
            .sum::<usize>(),
        vertices
    );

    let mut cold = dynamic_scene(16, false);

    cold.settings.max_mesh_vertices = 1;

    let patch = pollster::block_on(
        Input::capture(&cold, Domain::Body(cold.bodies[0].id))
            .unwrap()
            .prepare_local(vec![IVec3::new(3, 7, 1)], &mut cache),
    )
    .unwrap();

    assert!(matches!(patch, Patch::Unchanged));

    assert!(
        cold.bodies[0]
            .geometry
            .meshes
            .values()
            .all(|mesh| mesh.vertices().is_none())
    );
}

// Prepares several removals from the same original object. Checks that removals
// far apart can both be applied, while a nearby removal based on an outdated
// version is rejected rather than overwriting a change that has already happened.
#[test]
fn independent_cold_local_patches_merge_and_conflicting_patches_are_rejected() {
    let mut scene = dynamic_scene(32, false);

    let id = scene.bodies[0].id;

    let mut cache = Connectivity::default();

    let mut prepare = |x| {
        pollster::block_on(
            Input::capture(&scene, Domain::Body(id))
                .unwrap()
                .prepare_local(vec![IVec3::new(x, 3, 1)], &mut cache),
        )
        .unwrap()
    };

    let mut left = prepare(3);

    let mut conflict = prepare(4);

    let mut right = prepare(27);

    left.publish(&mut scene).unwrap();

    right.publish(&mut scene).unwrap();

    assert!(matches!(
        conflict.publish(&mut scene),
        Err(SceneError::StaleEdit)
    ));

    assert!(
        scene.bodies[0]
            .geometry
            .voxel(IVec3::new(3, 3, 1))
            .is_empty()
    );

    assert!(
        scene.bodies[0]
            .geometry
            .voxel(IVec3::new(27, 3, 1))
            .is_empty()
    );

    assert!(
        !scene.bodies[0]
            .geometry
            .voxel(IVec3::new(4, 3, 1))
            .is_empty()
    );

    scene.render_geometry().unwrap();
}

// Disturbs the bottom of an eight-object stack. Checks that every object in the
// stack is marked to be checked for movement, while a distant object stays at rest.
#[test]
fn wake_propagates_through_long_stacks_without_waking_remote_bodies() {
    let fixture = dynamic_scene(8, false);

    let mut body = fixture.bodies[0].clone();

    body.sleeping = true;
    body.translation = Vec3::ZERO;

    let height = body.geometry.max.y;

    let mut bodies: Vec<_> = (0..8)
        .map(|index| Body {
            id: index + 1,
            translation: Vec3::Y * (index as f32 * height),
            ..body.clone()
        })
        .collect();

    bodies.push(Body {
        id: 9,
        translation: Vec3::X * 128.0,
        ..body
    });

    let contacts = dynamics::Contacts::new(&bodies);

    contacts.wake(&mut bodies, Vec3::ZERO, Vec3::splat(0.01), 0.0);

    assert!(bodies[..8].iter().all(|body| !body.sleeping));

    assert!(bodies[8].sleeping);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(32))]

    // Arranges resting objects in many randomly chosen positions. Checks that the
    // faster way of finding objects affected by a disturbance produces exactly the
    // same result as checking every object, including chains of touching objects.
    #[test]
    fn contact_index_matches_transitive_full_scan(
        positions in proptest::collection::vec((-16i32..16, -16i32..16, -16i32..16), 1..64),
        query in (-16i32..16, -16i32..16, -16i32..16),
    ) {
        let fixture = dynamic_scene(8, false);
        let mut bodies: Vec<_> = positions.into_iter().enumerate().map(|(index, (x, y, z))| Body {
            id: index as u64 + 1,
            translation: IVec3::new(x, y, z).as_vec3() * VOXEL_SIZE,
            sleeping: true,
            ..fixture.bodies[0].clone()
        }).collect();
        let mut expected = bodies.clone();
        let min = IVec3::new(query.0, query.1, query.2).as_vec3() * VOXEL_SIZE;
        let max = min + Vec3::splat(VOXEL_SIZE);
        let slop = fixture.settings.simulation.contact_slop;
        let mut pending = vec![(min, max)];
        let mut visited = BTreeSet::new();

        while let Some((min, max)) = pending.pop() {
            for (index, body) in expected.iter_mut().enumerate() {
                let body_min = body.translation + body.geometry.min;
                let body_max = body.translation + body.geometry.max;
                let padding = Vec3::splat(slop + f32::EPSILON);
                if !visited.contains(&index) && body_min.cmple(max + padding).all() && body_max.cmpge(min - padding).all() {
                    visited.insert(index);
                    body.sleeping = false;
                    pending.push((body_min, body_max));
                }
            }
        }

        dynamics::Contacts::new(&bodies).wake(&mut bodies, min, max, slop);
        proptest::prop_assert_eq!(bodies.iter().map(|body| body.sleeping).collect::<Vec<_>>(), expected.iter().map(|body| body.sleeping).collect::<Vec<_>>());
    }
}

// Fills the allowed waiting list and checks that another request is refused.
// Then changes the object while the accepted removals are being prepared, and
// checks that those requests are retried and completed instead of being lost.
#[test]
fn scheduler_bounds_pending_work_and_retries_conflicting_edits() {
    let mut scene = dynamic_scene(32, false);

    let id = scene.bodies[0].id;

    let mut edits = SceneEdits::new(
        &scene,
        EditSettings {
            workers: 4,
            max_pending: 2,
            max_batch: 1,
        },
    )
    .unwrap();

    for x in [3, 27] {
        assert!(edits.queue_remove(Target::Dynamic {
            body: id,
            voxel: IVec3::new(x, 3, 1)
        }));
    }

    assert!(!edits.queue_remove(Target::Dynamic {
        body: id,
        voxel: IVec3::new(4, 3, 1)
    }));

    edits.update(&mut scene).unwrap();

    scene
        .remove(Target::Dynamic {
            body: id,
            voxel: IVec3::new(4, 3, 1),
        })
        .unwrap();

    let outcomes = finish(&mut scene, &mut edits);

    assert_eq!(outcomes.len(), 2);

    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));

    for x in [3, 4, 27] {
        assert!(
            scene.bodies[0]
                .geometry
                .voxel(IVec3::new(x, 3, 1))
                .is_empty()
        );
    }
}

#[test]
fn transferred_flight_and_pending_edits_follow_moving_repeatedly_split_bodies() {
    let mut config = settings();

    config.world.bounds.min.y = 0;

    let mut scene = Scene::new(config).unwrap();

    place(
        &mut scene,
        (0..16)
            .map(|y| (IVec3::new(-16, y, 0), voxel(1)))
            .chain((-16..48).map(|x| (IVec3::new(x, 16, 0), voxel(1)))),
    );

    let mut edits = SceneEdits::new(
        &scene,
        EditSettings {
            workers: 2,
            max_pending: 8,
            max_batch: 1,
        },
    )
    .unwrap();

    let target = IVec3::new(43, 16, 0);

    assert!(edits.queue_remove(Target::Static(target)));

    edits.update(&mut scene).unwrap();

    assert_eq!(edits.flights.len(), 1);

    let mut patch = pollster::block_on(
        Input::capture(&scene, Domain::Static)
            .unwrap()
            .prepare(vec![IVec3::new(-16, 15, 0)], &mut Connectivity::default()),
    )
    .unwrap();

    let replacements = patch.publish(&mut scene).unwrap();

    assert_eq!(replacements.len(), 1);

    let translation = replacements[0].translation;

    edits.retarget(&scene, Domain::Static, &replacements);

    assert!(
        edits.flights.is_empty(),
        "transferred static flight must be cancelled before publication"
    );

    assert_eq!(edits.pending(), 1);

    assert!(matches!(
        Target::from(edits.pending[0]),
        Target::Dynamic { .. }
    ));

    let body = replacements[0].id;

    let local = |x| IVec3::new(x, 16, 0) - (translation / VOXEL_SIZE).round().as_ivec3();

    for x in [0, 24, 10, 35] {
        assert!(edits.queue_remove(Target::Dynamic {
            body,
            voxel: local(x)
        }));
    }

    let deadline = Instant::now() + Duration::from_secs(5);

    let mut outcomes = Vec::new();

    let mut moved = false;

    while edits.pending() != 0 {
        scene.advance(Duration::from_millis(2)).unwrap();

        moved |= scene
            .bodies
            .iter()
            .any(|body| body.translation != translation);

        outcomes.extend(edits.update(&mut scene).unwrap());

        assert!(Instant::now() < deadline);

        std::thread::yield_now();
    }

    assert!(moved);

    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));

    assert_eq!(
        outcomes
            .iter()
            .map(|outcome| outcome.targets.len())
            .sum::<usize>(),
        5
    );

    assert_eq!(scene.bodies.len(), 6);

    let actual: BTreeSet<_> = scene
        .bodies
        .iter()
        .flat_map(|body| body.geometry.occupied().map(|(p, _)| p.to_array()))
        .collect();

    let expected: BTreeSet<_> = (-16..48)
        .filter(|x| ![0, 10, 24, 35, 43].contains(x))
        .map(|x| local(x).to_array())
        .collect();

    assert_eq!(actual, expected);

    assert_eq!(
        scene
            .bodies
            .iter()
            .map(|body| body.geometry.occupied().count())
            .sum::<usize>(),
        expected.len()
    );

    assert!(edits.accepted.is_empty() && edits.pending.is_empty() && edits.flights.is_empty());

    for y in 0..15 {
        assert_eq!(scene.world.resident_voxel(IVec3::new(-16, y, 0)), voxel(1));
    }
}

fn assert_body_reference(scene: &Scene, expected: &BTreeMap<[i32; 3], crate::world::Voxel>) {
    let geometry = &scene.bodies[0].geometry;

    assert_eq!(
        geometry
            .occupied()
            .map(|(p, v)| (p.to_array(), v))
            .collect::<BTreeMap<_, _>>(),
        *expected
    );

    let min = expected
        .keys()
        .fold(IVec3::splat(i32::MAX), |a, p| a.min(IVec3::from_array(*p)));

    let max = expected.keys().fold(IVec3::splat(i32::MIN), |a, p| {
        a.max(IVec3::from_array(*p) + IVec3::ONE)
    });

    assert_eq!(geometry.min, min.as_vec3() * VOXEL_SIZE);

    assert_eq!(geometry.max, max.as_vec3() * VOXEL_SIZE);

    let mass: f32 = expected
        .values()
        .map(|v| scene.world.material(v.material).unwrap().density * VOXEL_SIZE.powi(3))
        .sum();

    assert!((geometry.mass - mass).abs() < mass * 1e-5);

    let bottom: BTreeSet<_> = expected
        .keys()
        .filter(|p| !expected.contains_key(&(IVec3::from_array(**p) - IVec3::Y).to_array()))
        .copied()
        .collect();

    let actual: Vec<_> = geometry
        .bottom_regions()
        .flat_map(|(_, cells)| cells.iter().map(|p| p.to_array()))
        .collect();

    assert_eq!(actual.len(), bottom.len());

    assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), bottom);

    let rebuilt = pollster::block_on(crate::scene::body::BodyGeometry::new(
        &mut Connectivity::default(),
        &scene.world,
        geometry.leaves.clone(),
        scene.settings.render_leaf_edge,
        None,
    ))
    .unwrap();

    for (&key, source) in &geometry.meshes {
        let actual = source
            .resolve(
                key,
                scene.settings.render_leaf_edge,
                |key| geometry.leaves.get(&key).map(Arc::as_ref),
                &scene.world,
                usize::MAX,
            )
            .unwrap();

        let expected = rebuilt.meshes[&key]
            .resolve(
                key,
                scene.settings.render_leaf_edge,
                |key| rebuilt.leaves.get(&key).map(Arc::as_ref),
                &scene.world,
                usize::MAX,
            )
            .unwrap();

        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&actual.vertices),
            bytemuck::cast_slice::<_, u8>(&expected.vertices)
        );
    }
}

#[test]
fn merged_local_geometry_matches_voxel_mass_bounds_contacts_and_rebuilt_meshes() {
    let mut config = settings();

    config.world.bounds.min.y = 0;

    let mut fixture = Scene::new(config).unwrap();

    place(
        &mut fixture,
        (1..48)
            .flat_map(|x| {
                (16..28).flat_map(move |y| (0..4).map(move |z| (IVec3::new(x, y, z), voxel(1))))
            })
            .chain([(
                IVec3::new(0, 19, 1),
                crate::world::Voxel::new(crate::world::MaterialId(2), crate::world::OwnerId(1)),
            )]),
    );

    assert_eq!(fixture.bodies.len(), 1);

    for warm in [false, true] {
        for reverse in [false, true] {
            let mut scene = fixture.clone();

            if warm {
                scene.render_geometry().unwrap();
            }

            let id = scene.bodies[0].id;

            let mut expected: BTreeMap<_, _> = scene.bodies[0]
                .geometry
                .occupied()
                .map(|(p, v)| (p.to_array(), v))
                .collect();

            let mut patches: Vec<_> = [IVec3::new(0, 3, 1), IVec3::new(35, 7, 1)]
                .into_iter()
                .map(|p| {
                    expected.remove(&p.to_array()).unwrap();

                    let patch = pollster::block_on(
                        Input::capture(&scene, Domain::Body(id))
                            .unwrap()
                            .prepare_local(vec![p], &mut Connectivity::default()),
                    )
                    .unwrap();

                    assert!(matches!(patch, Patch::Local(_)));

                    patch
                })
                .collect();

            if reverse {
                patches.reverse();
            }

            for patch in &mut patches {
                patch.publish(&mut scene).unwrap();
            }

            assert_body_reference(&scene, &expected);
        }
    }
}

#[test]
fn merged_mesh_budget_failure_is_atomic_and_retryable() {
    let fixture = dynamic_scene(48, false);

    let id = fixture.bodies[0].id;

    let targets = [IVec3::new(3, 3, 1), IVec3::new(35, 3, 1)];

    let count = |scene: &Scene| {
        scene
            .render_geometry()
            .unwrap()
            .iter()
            .map(|m| m.mesh.vertices.len())
            .sum::<usize>()
    };

    let mut single = fixture.clone();

    single
        .remove(Target::Dynamic {
            body: id,
            voxel: targets[0],
        })
        .unwrap();

    let limit = count(&single);

    let mut combined = single.clone();

    combined
        .remove(Target::Dynamic {
            body: id,
            voxel: targets[1],
        })
        .unwrap();

    assert!(count(&combined) > limit);

    for reverse in [false, true] {
        let mut scene = fixture.clone();

        scene.settings.max_mesh_vertices = limit;

        let mut patches: Vec<_> = targets
            .into_iter()
            .map(|p| {
                pollster::block_on(
                    Input::capture(&scene, Domain::Body(id))
                        .unwrap()
                        .prepare_local(vec![p], &mut Connectivity::default()),
                )
                .unwrap()
            })
            .collect();

        assert!(patches.iter().all(|p| matches!(p, Patch::Local(_))));

        if reverse {
            patches.reverse();
        }

        patches[0].publish(&mut scene).unwrap();

        scene.bodies[0].sleeping = true;

        let before = scene.bodies[0].clone();

        let world = scene.snapshot();

        let next = scene.next_body;

        assert!(matches!(
            patches[1].publish(&mut scene),
            Err(SceneError::Limit("resident mesh vertices"))
        ));

        assert!(Arc::ptr_eq(&scene.bodies[0].geometry, &before.geometry));

        assert!(Arc::ptr_eq(&scene.world.root, &world.root));

        assert_eq!(scene.bodies[0].translation, before.translation);

        assert_eq!(scene.bodies[0].velocity, before.velocity);

        assert!(scene.bodies[0].sleeping);

        assert_eq!(scene.next_body, next);

        scene.settings.max_mesh_vertices = 1_000_000;

        let p = targets[usize::from(!reverse)];

        let mut retry = pollster::block_on(
            Input::capture(&scene, Domain::Body(id))
                .unwrap()
                .prepare_local(vec![p], &mut Connectivity::default()),
        )
        .unwrap();

        retry.publish(&mut scene).unwrap();

        let expected = combined.bodies[0]
            .geometry
            .occupied()
            .map(|(p, v)| (p.to_array(), v))
            .collect();

        assert_body_reference(&scene, &expected);
    }
}
