use capy_client::city::{City, CityConfig};
use engine::{ApplicationResult, Engine, Vec2, Vec3, View, ui::TextStyle};
use std::time::Instant;

pub async fn run(mut app: Engine) -> ApplicationResult<()> {
    let city = City::new(CityConfig::default())?;

    let mut camera = city.camera;

    let mut controls = crate::controls::bindings();

    let mut yaw = camera.direction.x.atan2(-camera.direction.z);

    let mut pitch = camera.direction.y.asin();

    let mut last = Instant::now();

    while let Some(mut frame) = app.next_frame().await? {
        let look = crate::controls::look(frame.take_captured_mouse_delta()?);

        yaw += look.x;
        pitch = (pitch + look.y).clamp(-1.5, 1.5);
        camera.direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        )
        .normalize();

        while let Some(tick) = frame.next_input()? {
            let state = controls.update(&tick.input, tick.delta);

            let movement = state.axis2(crate::controls::Action::Move);

            let speed = if state.pressed(crate::controls::Action::Sprint) {
                50.0
            } else {
                15.0
            };

            let right = camera.direction.cross(Vec3::Y).normalize();

            camera.position += (camera.direction * movement.y + right * movement.x)
                * speed
                * tick.delta.as_secs_f32();
        }

        let now = Instant::now();

        let elapsed = now.duration_since(last).as_secs_f32();

        last = now;

        frame.canvas().text(
            format!(
                "City renderer | {:.1} ms | WASD to fly, Shift for speed",
                elapsed * 1000.0
            ),
            Vec2::splat(16.0),
            &TextStyle::default(),
        );

        frame.present(View::new(&city.scene, camera))?;
    }

    Ok(())
}
