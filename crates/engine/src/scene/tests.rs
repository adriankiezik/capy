#![allow(clippy::unwrap_used)]

use super::*;
use crate::world::{self, MaterialId, OwnerId, StructureId, VOXEL_SIZE, Voxel, WorldError};
use glam::{IVec3, Vec3};
use proptest::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use world::tests::voxel;

pub(crate) fn settings() -> SceneSettings {
    SceneSettings {
        world: world::tests::settings(),
        visuals: VisualSettings::default(),
        simulation: SimulationSettings::default(),
        render_leaf_edge: 8,
        max_mesh_vertices: 1_000_000,
        max_bodies: 128,
    }
}

pub(crate) fn place(scene: &mut Scene, voxels: impl IntoIterator<Item = (IVec3, Voxel)>) {
    let mut transaction = scene.transaction();

    for (p, value) in voxels {
        transaction.place(p, value).unwrap();
    }

    scene.commit(transaction).unwrap();
}

fn structure_settings() -> SceneSettings {
    let mut config = settings();

    config.world.bounds.min.y = -8;

    for owner in &mut config.world.owners[1..] {
        owner.structure = StructureId(2);
    }

    config
}

fn bridge(config: SceneSettings) -> Scene {
    let mut scene = Scene::new(config).unwrap();

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

fn body_voxels(scene: &Scene) -> BTreeMap<[i32; 3], Voxel> {
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
        (body.translation.y + body.geometry.min.y - scene.settings.simulation.contact_slop).abs()
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

    let mut scene = Scene::new(config).unwrap();

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

fn assert_unchanged(
    scene: &Scene,
    world: &world::WorldRead,
    meshes: &geometry::MeshSources,
    bodies: &[Body],
) {
    assert!(Arc::ptr_eq(&scene.world.root, &world.root));

    assert_eq!(scene.meshes.len(), meshes.len());

    for (key, source) in meshes {
        assert!(Arc::ptr_eq(&scene.meshes[key], source));
    }

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

        scene.render_geometry().unwrap();

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

        let meshes = scene.meshes.clone();

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

            let meshes = scene.meshes.clone();

            let bodies = scene.bodies.clone();

            scene.settings.simulation.max_collision_probes = 1;

            assert!(matches!(
                scene.advance(Duration::from_millis(100)),
                Err(SceneError::Limit(_))
            ));

            assert_unchanged(&scene, &world, &meshes, &bodies);

            scene.settings.simulation.max_collision_probes = 1_000_000;

            scene.advance(Duration::from_millis(100)).unwrap();

            assert!(scene.bodies.iter().all(|b| b.translation.y < 0.0));
        } else {
            let result = scene.commit(transaction);

            match failure {
                "support" => assert!(matches!(result, Err(SceneError::Limit("support scan")))),
                "bodies" => assert!(matches!(result, Err(SceneError::Limit("dynamic bodies")))),
                "storage" => assert!(matches!(
                    result,
                    Err(SceneError::World(WorldError::Limit("storage leaves")))
                )),
                _ => unreachable!(),
            }

            assert_unchanged(&scene, &world, &meshes, &bodies);

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

type Faces = BTreeMap<([i32; 3], [i32; 3], [u32; 3]), usize>;

fn expected_faces(scene: &Scene) -> Faces {
    let mut faces = BTreeMap::new();

    let mut groups = vec![
        scene
            .settings
            .world
            .owners
            .iter()
            .flat_map(|owner| scene.world.owner_voxels(owner.id))
            .map(|(p, voxel)| (p.to_array(), voxel))
            .collect::<BTreeMap<_, _>>(),
    ];

    groups.extend(scene.bodies.iter().map(|body| {
        body.geometry
            .occupied()
            .map(|(p, voxel)| {
                let p = (p.as_vec3() + body.translation / VOXEL_SIZE)
                    .round()
                    .as_ivec3();

                (p.to_array(), voxel)
            })
            .collect()
    }));

    for group in groups {
        for (&p, voxel) in &group {
            let p = IVec3::from_array(p);

            for normal in [
                IVec3::X,
                -IVec3::X,
                IVec3::Y,
                -IVec3::Y,
                IVec3::Z,
                -IVec3::Z,
            ] {
                if !group.contains_key(&(p + normal).to_array()) {
                    let color = scene
                        .world
                        .material(voxel.material)
                        .unwrap()
                        .color
                        .map(f32::to_bits);

                    faces.insert((p.to_array(), normal.to_array(), color), 1);
                }
            }
        }
    }

    faces
}

fn corner_ao(p: IVec3, normal: IVec3, corner: IVec3, occupied: impl Fn(IVec3) -> bool) -> f32 {
    let axis = (0..3).find(|&axis| normal[axis] != 0).unwrap();

    let mut sides = [IVec3::ZERO; 2];

    for (index, axis) in (0..3).filter(|&a| a != axis).enumerate() {
        sides[index][axis] = if corner[axis] == p[axis] { -1 } else { 1 };
    }

    let side_count = sides
        .iter()
        .filter(|&&side| occupied(p + normal + side))
        .count();

    let blocked = if side_count == 2 {
        3
    } else {
        side_count + usize::from(occupied(p + normal + sides[0] + sides[1]))
    };

    (3 - blocked) as f32 / 3.0
}

fn assert_face_ao(
    quad: &[super::Vertex],
    origin: Vec3,
    p: IVec3,
    normal: IVec3,
    occupied: impl Fn(IVec3) -> bool,
) {
    let axes: Vec<_> = (0..3).filter(|&axis| normal[axis] == 0).collect();

    for a in 0..=1 {
        for b in 0..=1 {
            let mut corner = p + normal.max(IVec3::ZERO);

            corner[axes[0]] += a;
            corner[axes[1]] += b;

            let q = glam::Vec2::new(corner[axes[0]] as f32, corner[axes[1]] as f32);

            let expected = corner_ao(p, normal, corner, &occupied);

            let mut found = false;

            for triangle in quad.chunks_exact(3) {
                let xy: Vec<_> = triangle
                    .iter()
                    .map(|v| {
                        let p = (Vec3::from_array(v.position) + origin) / VOXEL_SIZE;

                        glam::Vec2::new(p[axes[0]], p[axes[1]])
                    })
                    .collect();

                let area = (xy[1] - xy[0]).perp_dot(xy[2] - xy[0]);

                let w = [
                    (xy[1] - q).perp_dot(xy[2] - q) / area,
                    (xy[2] - q).perp_dot(xy[0] - q) / area,
                    (xy[0] - q).perp_dot(xy[1] - q) / area,
                ];

                if w.iter().all(|&w| w >= -1e-5) {
                    let actual: f32 = w.iter().zip(triangle).map(|(w, v)| w * v.occlusion).sum();

                    assert!(
                        (actual - expected).abs() < 1e-5,
                        "AO at {p:?}, {normal:?}, corner {corner:?}: {actual} != {expected}"
                    );

                    found = true;
                }
            }

            assert!(found);
        }
    }
}

fn mesh_faces(scene: &Scene) -> Faces {
    let mut faces = BTreeMap::new();

    let groups = std::iter::repeat_n(None, scene.meshes.len()).chain(
        scene
            .bodies
            .iter()
            .flat_map(|body| std::iter::repeat_n(Some(body), body.geometry.meshes.len())),
    );

    for (instance, body) in scene.render_geometry().unwrap().into_iter().zip(groups) {
        let mesh = instance.mesh;

        assert_eq!(mesh.vertices.len() % 6, 0);

        for quad in mesh.vertices.chunks_exact(6) {
            let normal = Vec3::from_array(quad[0].normal);

            assert_eq!(normal.abs().element_sum(), 1.0);

            let axis = (0..3).find(|&axis| normal[axis] != 0.0).unwrap();

            let u = (axis + 1) % 3;

            let v = (axis + 2) % 3;

            let mut min = IVec3::splat(i32::MAX);

            let mut max = IVec3::splat(i32::MIN);

            let mut triangles = Vec::new();

            let mut doubled_area = 0.0;

            for triangle in quad.chunks_exact(3) {
                let positions: Vec<_> = triangle
                    .iter()
                    .map(|vertex| {
                        assert_eq!(vertex.normal, quad[0].normal);

                        assert_eq!(vertex.color, quad[0].color);

                        let local = Vec3::from_array(vertex.position);

                        assert!(local.cmpge(mesh.min).all() && local.cmple(mesh.max).all());

                        let p = (local + instance.world_origin) / VOXEL_SIZE;

                        assert!((p - p.round()).abs().max_element() < 1e-3);

                        let p = p.round().as_ivec3();

                        min = min.min(p);
                        max = max.max(p);

                        p
                    })
                    .collect();

                let area = (positions[1] - positions[0])
                    .as_vec3()
                    .cross((positions[2] - positions[0]).as_vec3())
                    .dot(normal);

                assert!(area > 0.0);

                doubled_area += area;

                triangles.push(
                    positions
                        .iter()
                        .map(|p| p.to_array())
                        .collect::<BTreeSet<_>>(),
                );
            }

            assert_eq!(min[axis], max[axis]);

            assert_eq!(
                doubled_area,
                (2 * (max[u] - min[u]) * (max[v] - min[v])) as f32
            );

            assert_ne!(triangles[0], triangles[1]);

            let shared: Vec<_> = triangles[0].intersection(&triangles[1]).collect();

            assert_eq!(shared.len(), 2);

            assert_eq!((shared[0][u] - shared[1][u]).abs(), max[u] - min[u]);

            assert_eq!((shared[0][v] - shared[1][v]).abs(), max[v] - min[v]);

            for a in min[u]..max[u] {
                for b in min[v]..max[v] {
                    let mut p = min;

                    p[u] = a;
                    p[v] = b;
                    p[axis] -= i32::from(normal[axis] > 0.0);

                    assert_face_ao(quad, instance.world_origin, p, normal.as_ivec3(), |p| {
                        !body
                            .map_or_else(
                                || scene.world.resident_voxel(p),
                                |body| {
                                    body.geometry.voxel(
                                        p - (body.translation / VOXEL_SIZE).round().as_ivec3(),
                                    )
                                },
                            )
                            .is_empty()
                    });

                    *faces
                        .entry((
                            p.to_array(),
                            normal.as_ivec3().to_array(),
                            quad[0].color.map(f32::to_bits),
                        ))
                        .or_default() += 1;
                }
            }
        }
    }

    faces
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    // Builds many small arrangements of blocks and checks their visible surfaces against
    // a simple block-by-block count. Every exposed surface must appear exactly once, face
    // outward, and have the right color; hidden surfaces must not be drawn.
    #[test]
    fn greedy_mesh_matches_independent_visible_face_oracle(
        samples in prop::collection::btree_map((-2i32..3, -2i32..3, -2i32..3), (1u16..3, 1u64..4), 1..80),
        edge in prop::sample::select(vec![8, 16, 32]),
    ) {
        let mut config = settings();
        config.render_leaf_edge = edge;
        let mut scene = Scene::new(config).unwrap();
        place(&mut scene, samples.into_iter().map(|((x, y, z), (material, owner))| (IVec3::new(x, y, z), Voxel::new(MaterialId(material), OwnerId(owner)))));
        assert_eq!(mesh_faces(&scene), expected_faces(&scene));
    }
}

// Draws neighboring blocks on either side of an area boundary, removes one, and checks
// that the newly exposed face appears instead of reusing an outdated picture of the blocks.
// Also checks that drawing limits still apply to previously prepared surfaces and that
// adding the block back hides the shared face again.
#[test]
fn cached_meshes_follow_boundary_edits_and_enforce_resident_budget() {
    for edge in [8, 16, 32] {
        for boundary in [-edge, 0, edge] {
            let mut config = settings();

            config.render_leaf_edge = edge;
            config.world.bounds.min.y = 0;

            let mut scene = Scene::new(config.clone()).unwrap();

            let left = IVec3::new(boundary - 1, 0, 0);

            let right = IVec3::new(boundary, 0, 0);

            place(&mut scene, [(left, voxel(1)), (right, voxel(2))]);

            assert_eq!(mesh_faces(&scene), expected_faces(&scene));

            let old = scene.snapshot();

            let mut transaction = scene.transaction();

            transaction.remove(right).unwrap();

            scene.commit(transaction).unwrap();

            let mut fresh = Scene::new(config).unwrap();

            place(&mut fresh, [(left, voxel(1))]);

            assert_eq!(
                mesh_faces(&scene),
                mesh_faces(&fresh),
                "edge {edge}, boundary {boundary}"
            );

            assert_eq!(mesh_faces(&scene), expected_faces(&scene));

            assert_eq!(old.voxel(right).unwrap(), voxel(2));

            scene.settings.max_mesh_vertices = 35;

            assert!(matches!(scene.render_geometry(), Err(SceneError::Limit(_))));

            scene.settings.max_mesh_vertices = 36;

            assert_eq!(mesh_faces(&scene).len(), 6);

            place(&mut scene, [(right, voxel(2))]);

            scene.settings.max_mesh_vertices = 1_000_000;

            assert_eq!(mesh_faces(&scene), expected_faces(&scene));
        }
    }
}

#[test]
fn ao_corner_patterns_choose_correct_values_and_triangle_diagonals() {
    let p = IVec3::new(-1, 7, 8);

    let mut diagonals = [0usize; 2];

    for axis in 0..3 {
        let u = (axis + 1) % 3;

        let v = (axis + 2) % 3;

        for sign in [-1, 1] {
            let mut normal = IVec3::ZERO;

            normal[axis] = sign;

            for pattern in 0u16..256 {
                let mut cells = BTreeSet::from([p.to_array()]);

                let ring: Vec<_> = (-1..=1)
                    .flat_map(|a| {
                        (-1..=1).filter_map(move |b| (a != 0 || b != 0).then_some((a, b)))
                    })
                    .collect();

                for (bit, &(a, b)) in ring.iter().enumerate() {
                    if pattern & (1 << bit) != 0 {
                        let mut q = p + normal;

                        q[u] += a;
                        q[v] += b;

                        cells.insert(q.to_array());
                    }
                }

                let mut scene = Scene::new(settings()).unwrap();

                let mut transaction = scene.transaction();

                for &q in &cells {
                    transaction.place(IVec3::from_array(q), voxel(1)).unwrap();
                }

                scene.world = world::prepare(&scene.world, transaction).unwrap().0;
                scene.meshes = Arc::new(
                    cells
                        .iter()
                        .map(|&q| {
                            (
                                geometry::key(IVec3::from_array(q), 8),
                                Arc::new(geometry::MeshSource::default()),
                            )
                        })
                        .collect(),
                );

                let mut checked = 0;

                for instance in scene.render_geometry().unwrap() {
                    for quad in instance.mesh.vertices.chunks_exact(6) {
                        let positions: Vec<_> = quad
                            .iter()
                            .map(|v| {
                                ((Vec3::from_array(v.position) + instance.world_origin)
                                    / VOXEL_SIZE)
                                    .round()
                                    .as_ivec3()
                            })
                            .collect();

                        if quad[0].normal != normal.as_vec3().to_array()
                            || !positions.iter().all(|q| {
                                q[axis] == p[axis] + i32::from(sign > 0)
                                    && (p[u]..=p[u] + 1).contains(&q[u])
                                    && (p[v]..=p[v] + 1).contains(&q[v])
                            })
                        {
                            continue;
                        }

                        assert_face_ao(quad, instance.world_origin, p, normal, |q| {
                            cells.contains(&q.to_array())
                        });

                        let corners = [(0, 0), (1, 0), (1, 1), (0, 1)].map(|(a, b)| {
                            let mut q = p + normal.max(IVec3::ZERO);

                            q[u] += a;
                            q[v] += b;

                            q
                        });

                        let ao = corners
                            .map(|q| corner_ao(p, normal, q, |q| cells.contains(&q.to_array())));

                        let flip = ao[0] + ao[2] > ao[1] + ao[3] + 1e-5;

                        let shared: BTreeSet<_> = positions[..3]
                            .iter()
                            .filter(|q| positions[3..].contains(q))
                            .map(|q| q.to_array())
                            .collect();

                        let expected = if flip {
                            [corners[1], corners[3]]
                        } else {
                            [corners[0], corners[2]]
                        };

                        assert_eq!(
                            shared,
                            expected.map(|q| q.to_array()).into_iter().collect(),
                            "pattern {pattern}, axis {axis}, sign {sign}"
                        );

                        diagonals[usize::from(flip)] += 1;
                        checked += 1;
                    }
                }

                assert_eq!(checked, 1);
            }
        }
    }

    assert!(diagonals.iter().all(|&count| count > 0));
}

#[test]
fn diagonal_ao_boundary_edits_match_fresh_geometry_for_sync_and_queued_removal() {
    for edge in [8, 16, 32] {
        for boundary in [-edge, 0, edge] {
            let mut config = settings();

            config.render_leaf_edge = edge;
            config.world.bounds.min.y = 0;

            let mut fixture = Scene::new(config).unwrap();

            place(
                &mut fixture,
                (boundary - 2..=boundary + 1)
                    .flat_map(|x| {
                        (boundary - 2..=boundary + 1).map(move |z| (IVec3::new(x, 0, z), voxel(1)))
                    })
                    .chain([(IVec3::new(boundary, 1, boundary), voxel(1))]),
            );

            let receiver = geometry::key(IVec3::new(boundary - 1, 0, boundary - 1), edge);

            fixture.render_geometry().unwrap();

            let source = fixture.meshes[&receiver].clone();

            for queued in [false, true] {
                let mut scene = fixture.clone();

                let target = IVec3::new(boundary, 1, boundary);

                if queued {
                    let mut edits =
                        super::SceneEdits::new(&scene, super::EditSettings::default()).unwrap();

                    assert!(edits.queue_remove(Target::Static(target)));

                    let deadline = std::time::Instant::now() + Duration::from_secs(5);

                    while edits.pending() != 0 {
                        for outcome in edits.update(&mut scene).unwrap() {
                            outcome.result.unwrap();
                        }

                        assert!(std::time::Instant::now() < deadline);

                        std::thread::yield_now();
                    }
                } else {
                    scene.remove(Target::Static(target)).unwrap();
                }

                assert!(!Arc::ptr_eq(&source, &scene.meshes[&receiver]));

                assert_eq!(mesh_faces(&scene), expected_faces(&scene));

                let mut fresh = Scene::new(scene.settings.clone()).unwrap();

                place(
                    &mut fresh,
                    (boundary - 2..=boundary + 1).flat_map(|x| {
                        (boundary - 2..=boundary + 1).map(move |z| (IVec3::new(x, 0, z), voxel(1)))
                    }),
                );

                let actual = scene.render_geometry().unwrap();

                let expected = fresh.render_geometry().unwrap();

                assert_eq!(actual.len(), expected.len());

                for (a, b) in actual.iter().zip(expected) {
                    assert_eq!(
                        bytemuck::cast_slice::<_, u8>(&a.mesh.vertices),
                        bytemuck::cast_slice::<_, u8>(&b.mesh.vertices)
                    );
                }
            }
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
        let mut scene = Scene::new(settings()).unwrap();
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
    let mut scene = Scene::new(structure_settings()).unwrap();

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

    let mut scene = Scene::new(config).unwrap();

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

    scene.settings.simulation.max_collision_probes = 1;

    assert!(matches!(
        scene.raycast(center + Vec3::Y * 2.0, -Vec3::Y, 3.0),
        Err(SceneError::Limit("query steps"))
    ));

    for (origin, direction, distance) in [
        (center, Vec3::ZERO, 1.0),
        (Vec3::splat(f32::NAN), Vec3::Y, 1.0),
        (center, Vec3::Y, f32::INFINITY),
        (center, Vec3::Y, 0.0),
    ] {
        assert!(matches!(
            scene.raycast(origin, direction, distance),
            Err(SceneError::Invalid)
        ));
    }
}
