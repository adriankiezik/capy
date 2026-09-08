#![allow(clippy::expect_used)]

use super::{
    EditSettings, Scene, SceneEdits, SceneSettings, SimulationSettings, Target, VisualSettings,
    geometry,
};
use crate::{
    Aabb,
    world::{
        Material, MaterialId, Owner, OwnerId, StructureId, Transaction, VOXEL_SIZE, Voxel,
        VoxelBounds, WorldRead, WorldSettings,
    },
};
use divan::{Bencher, black_box, counter::ItemsCount};
use glam::{IVec3, Vec3};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

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
                },
                Owner {
                    id: OwnerId(2),
                    structure: StructureId(2),
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
                (0..6).flat_map(move |y| (2..6).map(move |x| IVec3::new(leaf * 8 + x, y, z)))
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

fn finish_edits(scene: &mut Scene, edits: &mut SceneEdits) {
    let deadline = Instant::now() + Duration::from_secs(10);

    while edits.pending() != 0 {
        for outcome in edits.update(scene).expect("valid edit scheduler") {
            outcome.result.expect("valid batched edit");
        }

        assert!(
            Instant::now() < deadline,
            "edit scheduler completed within its deadline"
        );

        std::thread::yield_now();
    }
}

fn edit_bodies(bencher: Bencher, count: i32, workers: usize) {
    let mut fixture = scene();

    place(
        &mut fixture,
        (0..count).flat_map(|body| {
            (0..8).flat_map(move |z| {
                (16..24).flat_map(move |y| (0..8).map(move |x| IVec3::new(body * 16 + x, y, z)))
            })
        }),
        2,
    );

    let settings = EditSettings {
        workers,
        max_pending: 4096,
        max_batch: 64,
    };

    let commands: Vec<_> = fixture
        .bodies
        .iter()
        .flat_map(|body| {
            let id = body.id;

            (0..8)
                .flat_map(move |z| {
                    (0..8).map(move |y| Target::Dynamic {
                        body: id,
                        voxel: IVec3::new(4, y, z),
                    })
                })
                .chain([Target::Dynamic {
                    body: id,
                    voxel: IVec3::new(6, 1, 1),
                }])
        })
        .collect();

    let run = |scene: &mut Scene, edits: &mut SceneEdits| {
        for &target in &commands {
            assert!(edits.queue_remove(target));
        }

        for outcome in edits.update(scene).expect("dispatch independent bodies") {
            outcome.result.expect("valid body edit");
        }

        scene
            .advance(Duration::from_millis(16))
            .expect("advance during preparation");

        let translation = scene.bodies[0].translation.y;

        finish_edits(scene, edits);

        assert_eq!(scene.bodies.len(), count as usize * 2);

        assert!(
            scene
                .bodies
                .iter()
                .all(|body| body.translation.y == translation)
        );

        assert!(
            (scene.stats().dynamic_mass - count as f32 * 447.0 * VOXEL_SIZE.powi(3)).abs() < 0.001
        );
    };

    bencher
        .counter(ItemsCount::new(commands.len() as u64))
        .with_inputs(|| {
            let scene = fixture.clone();

            let edits = SceneEdits::new(&scene, settings).expect("valid scheduler settings");

            (scene, edits)
        })
        .bench_local_refs(|(scene, edits)| run(scene, edits));
}

#[divan::bench(args = [1, 8, 32])]
fn edits_bodies_serial(bencher: Bencher, count: i32) {
    edit_bodies(bencher, count, 1);
}

#[divan::bench(args = [1, 8, 32])]
fn edits_bodies_parallel(bencher: Bencher, count: i32) {
    edit_bodies(bencher, count, 4);
}

struct EditTimings {
    output: Option<std::path::PathBuf>,
    updates: Vec<Duration>,
    publications: Vec<Duration>,
    commands: Vec<Duration>,
    peak: usize,
}

impl Default for EditTimings {
    fn default() -> Self {
        Self {
            output: std::env::var_os("CAPY_EDIT_METRICS").map(Into::into),
            updates: Vec::new(),
            publications: Vec::new(),
            commands: Vec::new(),
            peak: 0,
        }
    }
}

impl EditTimings {
    fn update(&mut self, elapsed: Duration, outcomes: &[super::EditOutcome]) {
        if self.output.is_some() {
            self.updates.push(elapsed);

            if !outcomes.is_empty() {
                self.publications.push(elapsed);
            }
        }
    }

    fn completed(&mut self, durations: impl IntoIterator<Item = Duration>) {
        if self.output.is_some() {
            self.commands.extend(durations);
        }
    }

    fn report(mut self, workload: &str) {
        let Some(path) = self.output else {
            return;
        };

        self.updates.sort_unstable();

        self.publications.sort_unstable();

        self.commands.sort_unstable();

        let percentile = |values: &[Duration]| {
            values
                .get(values.len() * 99 / 100)
                .copied()
                .unwrap_or_default()
                .as_nanos()
        };

        let maximum = |values: &[Duration]| values.last().copied().unwrap_or_default().as_nanos();

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("edit metrics output");

        use std::io::Write;

        if file.metadata().expect("edit metrics metadata").len() == 0 {
            writeln!(file, "workload,updates,commands,update_p99_ns,update_max_ns,publication_p99_ns,command_p99_ns,command_max_ns,peak_pending")
                .expect("edit metrics header");
        }

        writeln!(
            file,
            "{workload},{},{},{},{},{},{},{},{}",
            self.updates.len(),
            self.commands.len(),
            percentile(&self.updates),
            maximum(&self.updates),
            percentile(&self.publications),
            percentile(&self.commands),
            maximum(&self.commands),
            self.peak
        )
        .expect("edit metrics row");
    }
}

fn edits_local(bencher: Bencher, workers: usize, dynamic: bool, separated: bool, terrain: bool) {
    let mut timings = EditTimings::default();

    let mut fixture = scene();

    let height = if terrain { 2 } else { 8 };

    let base_y = if dynamic { 16 } else { 0 };

    let stride = if separated { 16 } else { 8 };

    place(
        &mut fixture,
        (0..64).flat_map(|chunk| {
            (0..8).flat_map(move |z| {
                (0..height).flat_map(move |y| {
                    (0..8).map(move |x| IVec3::new(chunk * stride + x, base_y + y, z))
                })
            })
        }),
        1,
    );

    assert_eq!(fixture.bodies.len(), usize::from(dynamic));

    fixture.render_geometry().expect("warm resident geometry");

    let commands: Vec<_> = (0..if terrain { 1 } else { 4 })
        .flat_map(|wave| {
            (0..64).flat_map(move |chunk| {
                (1..7).flat_map(move |z| {
                    (1..7).map(move |x| IVec3::new(chunk * stride + x, height - wave - 1, z))
                })
            })
        })
        .map(|voxel| {
            if dynamic {
                Target::Dynamic {
                    body: fixture.bodies[0].id,
                    voxel,
                }
            } else {
                Target::Static(voxel)
            }
        })
        .collect();

    bencher
        .counter(ItemsCount::new(commands.len() as u64))
        .with_inputs(|| {
            let scene = fixture.clone();

            let edits = SceneEdits::new(
                &scene,
                EditSettings {
                    workers,
                    max_pending: 1024,
                    max_batch: 64,
                },
            )
            .expect("valid local scheduler");

            (scene, edits)
        })
        .bench_local_refs(|(scene, edits)| {
            let deadline = Instant::now() + Duration::from_secs(10);

            let mut next = 0;

            let mut completed = 0;

            let mut accepted = std::collections::BTreeMap::new();

            let mut latencies = Vec::new();

            let mut peak = 0;

            while next < commands.len() || edits.pending() != 0 {
                for _ in 0..128 {
                    let Some(&target) = commands.get(next) else {
                        break;
                    };

                    if !edits.queue_remove(target) {
                        break;
                    }

                    assert!(edits.queue_remove(target));

                    let voxel = match target {
                        Target::Static(voxel) | Target::Dynamic { voxel, .. } => voxel,
                    };

                    accepted.insert(voxel.to_array(), Instant::now());

                    next += 1;
                }

                peak = peak.max(edits.pending());

                let start = Instant::now();

                let outcomes = edits.update(scene).expect("valid localized update");

                timings.update(start.elapsed(), &outcomes);

                for outcome in outcomes {
                    outcome.result.expect("valid local publication");

                    for target in outcome.targets {
                        let voxel = match target {
                            Target::Static(voxel) | Target::Dynamic { voxel, .. } => voxel,
                        };

                        latencies.push(
                            accepted
                                .remove(&voxel.to_array())
                                .expect("accepted command completes once")
                                .elapsed(),
                        );

                        completed += 1;
                    }
                }

                assert!(Instant::now() < deadline, "bounded queue recovery");

                std::thread::yield_now();
            }

            assert_eq!(completed, commands.len());

            assert!(accepted.is_empty());

            assert!(peak <= 1024);

            assert_eq!(scene.bodies.len(), usize::from(dynamic));

            for &target in &commands {
                let empty = match target {
                    Target::Static(voxel) => scene.world.resident_voxel(voxel).is_empty(),
                    Target::Dynamic { voxel, .. } => {
                        scene.bodies[0].geometry.voxel(voxel).is_empty()
                    }
                };

                assert!(empty);
            }

            let remaining = 64 * 8 * height * 8 - commands.len() as i32;

            if dynamic {
                let geometry = &scene.bodies[0].geometry;

                assert!((geometry.mass - remaining as f32 * VOXEL_SIZE.powi(3)).abs() < 0.001);

                assert_eq!(geometry.min, Vec3::ZERO);

                assert_eq!(geometry.max, Vec3::new(512.0, 8.0, 8.0) * VOXEL_SIZE);
            } else {
                assert_eq!(
                    scene.world.owner_voxels(OwnerId(1)).count(),
                    remaining as usize
                );
            }

            timings.completed(latencies);

            timings.peak = timings.peak.max(peak);
        });

    timings.report(&format!(
        "local_workers{workers}_dynamic{dynamic}_separated{separated}_terrain{terrain}"
    ));
}

#[divan::bench(args = [1, 4])]
fn edits_local_building(bencher: Bencher, workers: usize) {
    edits_local(bencher, workers, false, false, false);
}

#[divan::bench(args = [1, 4])]
fn edits_local_buildings(bencher: Bencher, workers: usize) {
    edits_local(bencher, workers, false, true, false);
}

#[divan::bench(args = [1, 4])]
fn edits_local_terrain(bencher: Bencher, workers: usize) {
    edits_local(bencher, workers, false, false, true);
}

#[divan::bench(args = [1, 4])]
fn edits_local_dynamic(bencher: Bencher, workers: usize) {
    edits_local(bencher, workers, true, false, false);
}

fn edits_scale(
    bencher: Bencher,
    leaves: usize,
    dynamic: bool,
    sleeping: usize,
    warm_contacts: bool,
) {
    let mut settings = scene().settings.clone();

    settings.world.bounds.max.z = 2048;
    settings.max_bodies = settings.max_bodies.max(sleeping + 1);

    let mut fixture = Scene::new(settings).expect("valid scale settings");

    let source = Arc::new(crate::world::Leaf::Uniform(Voxel::new(
        MaterialId(1),
        OwnerId(1),
    )));

    let sources: crate::world::LeafSources = (0..leaves)
        .map(|index| {
            (
                [(index % 64) as i32, 0, (index / 64) as i32],
                source.clone(),
            )
        })
        .collect();

    if dynamic {
        let geometry = pollster::block_on(super::body::BodyGeometry::new(
            &mut super::connectivity::Connectivity::default(),
            &fixture.world,
            sources,
            8,
            None,
        ))
        .expect("connected large body");

        fixture.bodies.push(super::Body {
            id: sleeping as u64 + 1,
            geometry,
            translation: Vec3::Y * 1.6,
            velocity: 0.0,
            sleeping: false,
        });

        fixture.next_body = sleeping as u64 + 2;
    } else {
        fixture.world = fixture
            .world
            .replace_sources(
                sources
                    .iter()
                    .map(|(&key, source)| (key, Some(source.clone())))
                    .collect(),
            )
            .expect("grounded scale world");
        fixture.meshes = Arc::new(
            sources
                .keys()
                .map(|&key| (key, Arc::new(geometry::MeshSource::default())))
                .collect(),
        );
    }

    if sleeping != 0 {
        let geometry = pollster::block_on(super::body::BodyGeometry::new(
            &mut super::connectivity::Connectivity::default(),
            &fixture.world,
            [([0; 3], source)].into_iter().collect(),
            8,
            None,
        ))
        .expect("sleeping body geometry");

        for index in 0..sleeping {
            fixture.bodies.push(super::Body {
                id: index as u64 + 1,
                geometry: geometry.clone(),
                translation: Vec3::new(
                    (index % 64) as f32 * 16.0,
                    0.0,
                    (index / 64 + 1) as f32 * 16.0,
                ) * VOXEL_SIZE,
                velocity: 0.0,
                sleeping: true,
            });
        }
    }

    fixture.bodies.sort_by_key(|body| body.id);

    fixture.render_geometry().expect("warm scale meshes");

    fixture
        .contacts
        .get_or_init(|| super::dynamics::Contacts::new(&fixture.bodies));

    let mut timings = EditTimings::default();

    bencher
        .counter(ItemsCount::new(64u64))
        .with_inputs(|| {
            let mut scene = fixture.clone();

            if !warm_contacts {
                scene.contacts = Arc::default();
            }

            let edits = SceneEdits::new(
                &scene,
                EditSettings {
                    workers: 4,
                    max_pending: 128,
                    max_batch: 64,
                },
            )
            .expect("valid scale scheduler");

            (scene, edits)
        })
        .bench_local_refs(|(scene, edits)| {
            let accepted = Instant::now();

            for chunk in 0..64 {
                let voxel = IVec3::new(chunk * 8 + 3, 7, 3);

                let target = if dynamic {
                    Target::Dynamic {
                        body: sleeping as u64 + 1,
                        voxel,
                    }
                } else {
                    Target::Static(voxel)
                };

                assert!(edits.queue_remove(target));
            }

            timings.peak = timings.peak.max(edits.pending());

            let deadline = Instant::now() + Duration::from_secs(10);

            while edits.pending() != 0 {
                let start = Instant::now();

                let outcomes = edits.update(scene).expect("scale edit update");

                timings.update(start.elapsed(), &outcomes);

                for outcome in outcomes {
                    outcome.result.expect("scale edit publication");

                    timings.completed(std::iter::repeat_n(
                        accepted.elapsed(),
                        outcome.targets.len(),
                    ));
                }

                assert!(Instant::now() < deadline, "scale edit deadline");

                std::thread::yield_now();
            }

            assert_eq!(scene.bodies.len(), usize::from(dynamic) + sleeping);

            assert_eq!(
                scene.bodies.iter().filter(|body| body.sleeping).count(),
                sleeping
            );

            if dynamic {
                assert_eq!(scene.bodies[sleeping].geometry.leaves.len(), leaves);

                assert!(
                    (scene.bodies[sleeping].geometry.mass
                        - (leaves * 512 - 64) as f32 * VOXEL_SIZE.powi(3))
                    .abs()
                        < 0.001
                );
            } else {
                assert_eq!(scene.world.leaf_count(), leaves);
            }

            for chunk in 0..64 {
                let voxel = IVec3::new(chunk * 8 + 3, 7, 3);

                assert!(if dynamic {
                    scene.bodies[sleeping].geometry.voxel(voxel).is_empty()
                } else {
                    scene.world.resident_voxel(voxel).is_empty()
                });
            }
        });

    timings.report(&format!(
        "scale_leaves{leaves}_dynamic{dynamic}_sleeping{sleeping}_warmcontacts{warm_contacts}"
    ));
}

#[divan::bench(args = [0, 64, 4096])]
fn edits_sleeping_bodies(bencher: Bencher, sleeping: usize) {
    edits_scale(bencher, 64, true, sleeping, true);
}

#[divan::bench(args = [0, 64, 4096])]
fn edits_sleeping_bodies_cold(bencher: Bencher, sleeping: usize) {
    edits_scale(bencher, 64, true, sleeping, false);
}

#[divan::bench(args = [64, 4096])]
fn edits_scale_static(bencher: Bencher, leaves: usize) {
    edits_scale(bencher, leaves, false, 0, true);
}

#[divan::bench(args = [64, 4096])]
fn edits_scale_dynamic(bencher: Bencher, leaves: usize) {
    edits_scale(bencher, leaves, true, 0, true);
}

#[divan::bench(args = [1, 4])]
fn edits_independent_support_cuts(bencher: Bencher, workers: usize) {
    let mut fixture = scene();

    place(
        &mut fixture,
        (0..32).flat_map(|tower| {
            (0..16)
                .map(move |y| IVec3::new(tower * 32, y, 0))
                .chain((0..8).flat_map(move |z| {
                    (16..24)
                        .flat_map(move |y| (0..8).map(move |x| IVec3::new(tower * 32 + x, y, z)))
                }))
        }),
        1,
    );

    assert!(fixture.bodies.is_empty());

    bencher
        .counter(ItemsCount::new(64u64))
        .with_inputs(|| {
            let scene = fixture.clone();

            let edits = SceneEdits::new(
                &scene,
                EditSettings {
                    workers,
                    max_pending: 128,
                    max_batch: 64,
                },
            )
            .expect("valid structural scheduler");

            (scene, edits)
        })
        .bench_local_refs(|(scene, edits)| {
            for tower in 0..32 {
                assert!(edits.queue_remove(Target::Static(IVec3::new(tower * 32, 0, 0))));

                assert!(edits.queue_remove(Target::Static(IVec3::new(tower * 32 + 4, 20, 4))));
            }

            finish_edits(scene, edits);

            assert_eq!(scene.bodies.len(), 32);

            assert_eq!(scene.world.leaf_count(), 0);

            assert!((scene.stats().dynamic_mass - 32.0 * 526.0 * VOXEL_SIZE.powi(3)).abs() < 0.001);
        });
}

#[divan::bench(args = [16, 128, 512])]
fn edits_static_batch(bencher: Bencher, length: i32) {
    let mut fixture = scene();

    place(
        &mut fixture,
        (0..length)
            .flat_map(|x| (0..4).flat_map(move |y| (0..4).map(move |z| IVec3::new(x, y, z)))),
        1,
    );

    bencher
        .counter(ItemsCount::new(length as u64))
        .with_inputs(|| {
            let scene = fixture.clone();

            let edits = SceneEdits::new(
                &scene,
                EditSettings {
                    workers: 4,
                    max_pending: length as usize,
                    max_batch: (length as usize).min(256),
                },
            )
            .expect("valid batch scheduler");

            (scene, edits)
        })
        .bench_local_refs(|(scene, edits)| {
            for x in 0..length {
                assert!(edits.queue_remove(Target::Static(IVec3::new(x, 1, 1))));
            }

            finish_edits(scene, edits);

            assert!(scene.bodies.is_empty());

            assert!(
                (0..length).all(|x| scene.world.resident_voxel(IVec3::new(x, 1, 1)).is_empty())
            );
        });
}

#[divan::bench(args = [8, 64, 512])]
fn edits_queued_support_cuts(bencher: Bencher, length: i32) {
    bencher
        .counter(ItemsCount::new(3u64))
        .with_inputs(|| {
            let (scene, _) = supported_beam(length);

            let edits = SceneEdits::new(
                &scene,
                EditSettings {
                    workers: 2,
                    max_pending: 3,
                    max_batch: 1,
                },
            )
            .expect("valid support scheduler");

            (scene, edits)
        })
        .bench_local_refs(|(scene, edits)| {
            for voxel in [
                IVec3::ZERO,
                IVec3::new(length / 2, 1, 0),
                IVec3::new(length - 1, 1, 0),
            ] {
                assert!(edits.queue_remove(Target::Static(voxel)));
            }

            finish_edits(scene, edits);

            assert_eq!(scene.bodies.len(), 2);

            assert!(
                (scene.stats().dynamic_mass - (length - 2) as f32 * VOXEL_SIZE.powi(3)).abs()
                    < 0.0001
            );
        });
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
