#![allow(clippy::unwrap_used)]

use super::*;
use crate::{
    scene::{
        Scene,
        tests::{place, settings},
    },
    world::tests::voxel,
};
use glam::{IVec3, Vec2, Vec3};
use std::time::Duration;

fn course() -> Scene {
    let mut scene = Scene::new(settings()).unwrap();

    place(
        &mut scene,
        (-20..=20).flat_map(|x| (-20..=20).map(move |z| (IVec3::new(x, -1, z), voxel(1)))),
    );

    place(
        &mut scene,
        (0..25).flat_map(|y| (-10..=10).map(move |z| (IVec3::new(10, y, z), voxel(1)))),
    );

    place(
        &mut scene,
        (-5..=5).flat_map(|x| (-5..=5).map(move |z| (IVec3::new(x, 22, z), voxel(1)))),
    );

    scene
}

fn player(settings: PlayerSettings) -> Player {
    Player::new(
        settings,
        PlayerPose {
            position: Vec3::new(0.0, 0.001, 0.0),
            ..PlayerPose::default()
        },
    )
    .unwrap()
}

// Makes the player jump beneath a low ceiling, land, jump again, and sprint into a wall.
// The player must not pass through solid surfaces or jump again while already in the air,
// but must be able to jump again after landing.
#[test]
fn obstacle_course_blocks_walls_and_ceiling_then_allows_another_grounded_jump() {
    let scene = course();

    let mut player = player(PlayerSettings::default());

    let dt = Duration::from_secs_f32(1.0 / 60.0);

    for _ in 0..5 {
        player.advance(&scene, PlayerInput::default(), dt).unwrap();
    }

    let ground = player.position().y;

    let mut peaks = Vec::new();

    for _ in 0..2 {
        player
            .advance(
                &scene,
                PlayerInput {
                    jump: true,
                    ..PlayerInput::default()
                },
                dt,
            )
            .unwrap();

        assert!(player.position().y > ground + 0.01);

        let mut with_air_jump = player.clone();

        let mut without_air_jump = player.clone();

        with_air_jump
            .advance(
                &scene,
                PlayerInput {
                    jump: true,
                    ..PlayerInput::default()
                },
                dt,
            )
            .unwrap();

        without_air_jump
            .advance(&scene, PlayerInput::default(), dt)
            .unwrap();

        assert_eq!(with_air_jump.position(), without_air_jump.position());

        let mut peak = player.position().y;

        for _ in 0..90 {
            player.advance(&scene, PlayerInput::default(), dt).unwrap();

            assert!(!scene.intersects(player.bounds()).unwrap());

            peak = peak.max(player.position().y);
        }

        assert!(
            (0.3..=0.40001).contains(&peak),
            "ceiling-limited jump peak {peak}"
        );

        assert!((player.position().y - ground).abs() < 1e-4);

        peaks.push(peak);
    }

    assert!((peaks[0] - peaks[1]).abs() < 1e-4);

    for _ in 0..60 {
        player
            .advance(
                &scene,
                PlayerInput {
                    movement: Vec2::X,
                    sprint: true,
                    jump: false,
                },
                dt,
            )
            .unwrap();

        assert!(!scene.intersects(player.bounds()).unwrap());

        assert!(player.bounds().max.x <= 1.0 + 1e-6);
    }

    assert!((player.position().x - 0.72).abs() < 1e-4);
}

// Checks that moving diagonally is no faster than moving straight. It then interrupts
// a move after one direction could already have been processed: the player must remain
// at the original position and continue moving afterward as if the failed move never happened.
#[test]
fn diagonal_speed_is_normalized_and_failed_partial_movement_rolls_back() {
    let scene = course();

    let mut straight = player(PlayerSettings::default());

    let mut diagonal = straight.clone();

    let dt = Duration::from_millis(20);

    straight
        .advance(
            &scene,
            PlayerInput {
                movement: Vec2::X,
                ..PlayerInput::default()
            },
            dt,
        )
        .unwrap();

    diagonal
        .advance(
            &scene,
            PlayerInput {
                movement: Vec2::ONE,
                ..PlayerInput::default()
            },
            dt,
        )
        .unwrap();

    let horizontal = |p: &Player| Vec2::new(p.position().x, p.position().z).length();

    assert!((horizontal(&straight) - horizontal(&diagonal)).abs() < 1e-6);

    assert!((horizontal(&straight) - 4.5 * dt.as_secs_f32()).abs() < 1e-6);

    let mut limited = player(PlayerSettings {
        max_collision_steps: 1,
        ..PlayerSettings::default()
    });

    let mut control = limited.clone();

    let before = limited.position();

    assert!(
        limited
            .advance(
                &scene,
                PlayerInput {
                    movement: Vec2::new(0.1, 0.9),
                    ..PlayerInput::default()
                },
                dt
            )
            .is_err()
    );

    assert_eq!(limited.position(), before);

    assert_eq!(limited.camera().direction, control.camera().direction);

    for _ in 0..20 {
        limited
            .advance(&scene, PlayerInput::default(), Duration::from_millis(10))
            .unwrap();

        control
            .advance(&scene, PlayerInput::default(), Duration::from_millis(10))
            .unwrap();

        assert_eq!(limited.position(), control.position());
    }
}
