#![allow(clippy::expect_used)]

use super::{Player, PlayerInput, PlayerPose, PlayerSettings};
use crate::{
    scene::{Scene, SceneSettings, SimulationSettings, VisualSettings},
    world::{
        Material, MaterialId, Owner, OwnerId, StructureId, Support, Voxel, VoxelBounds,
        WorldSettings,
    },
};
use divan::{Bencher, black_box};
use glam::{IVec3, Vec2, Vec3};
use std::time::Duration;

#[derive(Clone, Copy)]
enum Movement {
    Free,
    Grounded,
    Wall,
}

fn scene(movement: Movement) -> Scene {
    let mut scene = Scene::new(SceneSettings {
        world: WorldSettings {
            bounds: VoxelBounds {
                min: IVec3::ZERO,
                max: IVec3::new(64, 128, 64),
            },
            materials: vec![Material {
                id: MaterialId(1),
                color: [0.5; 3],
                density: 1.0,
            }],
            owners: vec![Owner {
                id: OwnerId(1),
                structure: StructureId(1),
                support: Support::Fixed,
            }],
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
    .expect("valid player scene");

    let mut transaction = scene.transaction();

    let voxel = Voxel::new(MaterialId(1), OwnerId(1));

    if !matches!(movement, Movement::Free) {
        for z in 0..32 {
            for x in 0..32 {
                transaction
                    .place(IVec3::new(x, 0, z), voxel)
                    .expect("valid floor");
            }
        }
    }

    if matches!(movement, Movement::Wall) {
        for z in 0..32 {
            for y in 1..24 {
                transaction
                    .place(IVec3::new(16, y, z), voxel)
                    .expect("valid wall");
            }
        }
    }

    scene
        .commit(transaction)
        .expect("valid player scene commit");

    scene
}

fn advance(bencher: Bencher, sprint: bool, movement: Movement) {
    let scene = scene(movement);

    let delta = Duration::from_secs_f64(1.0 / 60.0);

    let settings = PlayerSettings::default();

    let speed = settings.speed
        * if sprint {
            settings.sprint_multiplier
        } else {
            1.0
        };

    let mut player = Player::new(
        settings,
        PlayerPose {
            position: if matches!(movement, Movement::Free) {
                Vec3::new(1.31, 3.0, 1.0)
            } else {
                Vec3::new(1.31, 0.1001, 1.0)
            },
            ..PlayerPose::default()
        },
    )
    .expect("valid player fixture");

    if !matches!(movement, Movement::Free) {
        player
            .advance(&scene, PlayerInput::default(), delta)
            .expect("ground player");
    }

    assert!(
        !scene
            .intersects(player.bounds())
            .expect("valid player bounds")
    );

    let input = PlayerInput {
        movement: Vec2::X,
        sprint,
        jump: false,
    };

    let mut verification = player.clone();

    verification
        .advance(&scene, input, delta)
        .expect("valid verification step");

    let displacement = verification.position() - player.position();

    if matches!(movement, Movement::Wall) {
        assert!(displacement.x > 0.0 && displacement.x < 0.02);
    } else {
        assert!((displacement.x - speed * delta.as_secs_f32()).abs() < 0.0001);
    }

    if matches!(movement, Movement::Free) {
        assert!(displacement.y < 0.0);
    } else {
        assert!(displacement.y.abs() < 0.0001);
    }

    assert!(
        !scene
            .intersects(verification.bounds())
            .expect("valid resulting bounds")
    );

    bencher
        .with_inputs(|| player.clone())
        .bench_local_refs(|player| {
            player
                .advance(black_box(&scene), black_box(input), black_box(delta))
                .expect("valid player step");

            black_box(player.position());
        });
}

#[divan::bench(args = [false, true])]
fn advance_free(bencher: Bencher, sprint: bool) {
    advance(bencher, sprint, Movement::Free);
}

#[divan::bench(args = [false, true])]
fn advance_grounded(bencher: Bencher, sprint: bool) {
    advance(bencher, sprint, Movement::Grounded);
}

#[divan::bench(args = [false, true])]
fn advance_wall_collision(bencher: Bencher, sprint: bool) {
    advance(bencher, sprint, Movement::Wall);
}
