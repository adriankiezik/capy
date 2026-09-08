use crate::{
    controls::{self, Action},
    scene,
};
use engine::{
    ApplicationResult as Result, Engine, Vec2, Vec3, View,
    player::{Player, PlayerInput, PlayerPose, PlayerSettings},
    scene::{Hit, Target},
    ui::TextStyle,
};
use std::time::{Duration, Instant};

fn spawn_player() -> Result<Player> {
    Ok(Player::new(
        PlayerSettings::default(),
        PlayerPose {
            position: Vec3::new(0.0, 0.01, 6.0),
            yaw_radians: 0.0,
            pitch_radians: 0.0,
        },
    )?)
}

pub async fn run(mut app: Engine) -> Result<()> {
    let mut scene = scene::create()?;

    let mut player = spawn_player()?;

    let mut controls = controls::bindings();

    let mut selection = None;

    let mut fps_start = Instant::now();

    let mut frame_count = 0_u64;

    let mut fps = String::from("FPS: ...");

    while let Some(mut frame) = app.next_frame().await? {
        frame_count += 1;

        let elapsed = fps_start.elapsed();

        if elapsed >= Duration::from_millis(500) {
            fps = format!("FPS: {:.0}", frame_count as f64 / elapsed.as_secs_f64());
            fps_start = Instant::now();
            frame_count = 0;
        }

        while let Some(tick) = frame.next_tick()? {
            let actions = controls.update(&tick.input, tick.delta);

            if !tick.input.focused() {
                continue;
            }

            let look = actions.axis2(Action::Look);

            player.rotate_view(look.x, look.y)?;

            player.advance(
                &scene,
                PlayerInput {
                    movement: actions.axis2(Action::Move),
                    sprint: actions.pressed(Action::Sprint),
                    jump: actions.just_pressed(Action::Jump),
                },
                tick.delta,
            )?;

            if player.position().y < -6.0 {
                player = spawn_player()?;
            }

            let camera = player.camera();

            selection = scene.raycast(camera.position, camera.direction, 8.0)?;

            if actions.active(Action::Destroy)
                && let Some(Hit {
                    target: Target::Static(voxel),
                    ..
                }) = selection
            {
                let mut transaction = scene.transaction();

                transaction.remove(voxel)?;

                scene.commit(transaction)?;

                selection = scene.raycast(camera.position, camera.direction, 8.0)?;
            }
        }

        frame
            .canvas()
            .text(fps.as_str(), Vec2::splat(16.0), &TextStyle::default());

        frame.present(View::new(&scene, player.camera()).with_selection(selection))?;
    }

    Ok(())
}
