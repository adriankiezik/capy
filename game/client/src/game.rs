use crate::{
    config::Config,
    controls::{self, Action},
};
use capy_engine_protocol::message::PeerId;
use capy_protocol::{Command, GAME_ID, State};
use engine::{
    ApplicationResult as Result, Engine, Vec2, Vec3, View,
    camera::Camera,
    connection::{ClientSession, Connection},
    ui::TextStyle,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub async fn run(mut app: Engine, config: Config) -> Result<()> {
    let (connection, server) = if let Some(address) = config.connect {
        (
            Connection::<State>::connect(address, GAME_ID, Default::default())?,
            None,
        )
    } else {
        let address = crate::config::LOCAL_SERVER;

        let server = match std::net::TcpListener::bind(address) {
            Ok(listener) => Some(capy_server::host(listener)?),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => None,
            Err(error) => return Err(error.into()),
        };

        (
            Connection::<State>::connect(address, GAME_ID, Default::default())?,
            server,
        )
    };

    let mut controls = controls::bindings();

    let mut session = ClientSession::new(connection, crate::config::presentation());

    let mut peer = PeerId(0);

    let mut state: Option<State> = None;

    let mut previous_players = BTreeMap::<PeerId, Vec3>::new();

    let mut rendered_players = BTreeMap::<PeerId, Vec3>::new();

    let mut yaw = 0.0_f32;

    let mut pitch = 0.0_f32;

    let mut eye = Vec3::new(0.0, 1.6, 6.0);

    let mut previous_eye = eye;

    let mut target_eye = eye;

    let mut updated = Instant::now();

    let mut interval = Duration::from_millis(50);

    let mut fps_start = Instant::now();

    let mut frames = 0;

    let mut fps = String::from("Connecting...");

    while let Some(mut frame) = app.next_frame().await? {
        if server.as_ref().is_some_and(|server| server.finished()) {
            anyhow::bail!("embedded server stopped");
        }

        for _ in 0..32 {
            let Some(update) = session.poll()? else {
                break;
            };

            let game = update.state;

            peer = update.peer;

            anyhow::ensure!(game.valid(peer), "invalid player state");

            interval = update.interval;

            if update.reset {
                if let Some(player) = game.players.iter().find(|p| p.peer == peer) {
                    yaw = player.yaw;
                    pitch = player.pitch;
                    eye = Vec3::from_array(player.eye);
                    previous_eye = eye;
                    target_eye = eye;
                }

                previous_players = game
                    .players
                    .iter()
                    .map(|p| (p.peer, Vec3::from_array(p.position)))
                    .collect();
                rendered_players = previous_players.clone();
            } else {
                previous_players = rendered_players.clone();

                if let Some(player) = game.players.iter().find(|p| p.peer == peer) {
                    previous_eye = eye;
                    target_eye = Vec3::from_array(player.eye);
                }
            }

            updated = Instant::now();
            state = Some(game);
        }

        let look = controls::look(frame.take_captured_mouse_delta()?);

        yaw = (yaw + look.x).rem_euclid(std::f32::consts::TAU);
        pitch = (pitch + look.y).clamp(-1.55, 1.55);

        while let Some(tick) = frame.next_input()? {
            let actions = controls.update(&tick.input, tick.delta);

            let focused = tick.input.focused();

            if state.is_some() {
                session.send(Command {
                    movement: if focused {
                        actions.axis2(Action::Move).to_array()
                    } else {
                        [0.0; 2]
                    },
                    yaw,
                    pitch,
                    sprint: focused && actions.pressed(Action::Sprint),
                    jump: focused && actions.just_pressed(Action::Jump),
                    destroy: focused && actions.active(Action::Destroy),
                })?;
            }
        }

        eye = previous_eye.lerp(
            target_eye,
            (updated.elapsed().as_secs_f32() / interval.as_secs_f32()).min(1.0),
        );
        frames += 1;

        if fps_start.elapsed() >= Duration::from_millis(500) {
            fps = format!(
                "FPS: {:.0} | Players: {}",
                frames as f64 / fps_start.elapsed().as_secs_f64(),
                state.as_ref().map_or(0, |state| state.players.len())
            );
            frames = 0;
            fps_start = Instant::now();
        }

        frame
            .canvas()
            .text(&fps, Vec2::splat(16.0), &TextStyle::default());

        if let Some(scene) = session.replica_mut() {
            rendered_players.clear();

            let alpha = (updated.elapsed().as_secs_f32() / interval.as_secs_f32()).min(1.0);

            let objects: Vec<_> = state
                .as_ref()
                .into_iter()
                .flat_map(|s| s.players.iter())
                .filter(|p| p.peer != peer)
                .map(|player| {
                    let target = Vec3::from_array(player.position);

                    let position = previous_players
                        .get(&player.peer)
                        .copied()
                        .unwrap_or(target)
                        .lerp(target, alpha);

                    rendered_players.insert(player.peer, position);

                    engine::replica::RenderObject {
                        id: player.peer.0,
                        bounds: engine::Aabb {
                            min: position - Vec3::new(0.25, 0.0, 0.25),
                            max: position + Vec3::new(0.25, 1.8, 0.25),
                        },
                        color: [0.25, 0.45, 0.85],
                    }
                })
                .collect();

            scene.set_objects(&objects)?;

            let camera = Camera {
                position: eye,
                direction: Vec3::new(
                    yaw.sin() * pitch.cos(),
                    pitch.sin(),
                    -yaw.cos() * pitch.cos(),
                ),
                fov_radians: 75.0_f32.to_radians(),
                near_plane: 0.01,
            };

            let selection = scene.raycast(camera.position, camera.direction, 8.0)?;

            frame.present(View::new(scene, camera).with_selection(selection))?;
        }
    }

    drop(session);

    if let Some(server) = server {
        server.shutdown();

        server.join()?;
    }

    Ok(())
}
