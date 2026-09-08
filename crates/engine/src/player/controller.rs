use std::time::Duration;

use crate::{
    Aabb,
    player::{PlayerError, PlayerSettings, Result},
    scene::Scene,
    world::VOXEL_SIZE,
};
use glam::{Mat4, Vec2, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub direction: Vec3,
    pub fov_radians: f32,
    pub near_plane: f32,
}

impl Camera {
    pub(crate) fn matrix(self, aspect: f32, far: f32) -> Option<Mat4> {
        if !self.position.is_finite() {
            return None;
        }

        if !self.direction.is_finite() || !self.direction.is_normalized() {
            return None;
        }

        if self.direction.cross(Vec3::Y).length_squared() < f32::EPSILON {
            return None;
        }

        if !(0.0..std::f32::consts::PI).contains(&self.fov_radians) || self.fov_radians == 0.0 {
            return None;
        }

        if !self.near_plane.is_finite() || self.near_plane <= 0.0 {
            return None;
        }

        if !far.is_finite() || far <= self.near_plane {
            return None;
        }

        if !aspect.is_finite() || aspect <= 0.0 {
            return None;
        }

        let matrix =
            glam::camera::rh::proj::directx::perspective(
                self.fov_radians,
                aspect,
                self.near_plane,
                far,
            ) * glam::camera::rh::view::look_to_mat4(self.position, self.direction, Vec3::Y);

        matrix.is_finite().then_some(matrix)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerInput {
    pub movement: Vec2,
    pub sprint: bool,
    pub jump: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerPose {
    pub position: Vec3,
    pub yaw_radians: f32,
    pub pitch_radians: f32,
}

#[derive(Clone, Debug)]
pub struct Player {
    settings: PlayerSettings,
    position: Vec3,
    yaw: f32,
    pitch: f32,
    velocity: f32,
    grounded: bool,
}

impl Player {
    pub fn new(settings: PlayerSettings, pose: PlayerPose) -> Result<Self> {
        let PlayerPose {
            position,
            yaw_radians: yaw,
            pitch_radians: pitch,
        } = pose;

        if !position.is_finite()
            || !yaw.is_finite()
            || !pitch.is_finite()
            || [
                settings.radius,
                settings.height,
                settings.eye_height,
                settings.speed,
                settings.sprint_multiplier,
                settings.jump_speed,
                settings.gravity,
                settings.fov_radians,
                settings.near_plane,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
            || settings.eye_height > settings.height
            || settings.fov_radians >= std::f32::consts::PI
            || settings.near_plane >= settings.radius
            || settings.max_collision_steps == 0
            || !(1..=24).contains(&settings.collision_iterations)
        {
            return Err(PlayerError::Invalid);
        }

        Ok(Self {
            settings,
            position,
            yaw,
            pitch: pitch.clamp(-1.55, 1.55),
            velocity: 0.0,
            grounded: false,
        })
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn camera(&self) -> Camera {
        Camera {
            position: self.position + Vec3::Y * self.settings.eye_height,
            direction: Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.pitch.sin(),
                -self.yaw.cos() * self.pitch.cos(),
            ),
            fov_radians: self.settings.fov_radians,
            near_plane: self.settings.near_plane,
        }
    }

    pub fn rotate_view(&mut self, delta_yaw: f32, delta_pitch: f32) -> Result<()> {
        if !delta_yaw.is_finite() || !delta_pitch.is_finite() {
            return Err(PlayerError::Invalid);
        }

        self.yaw = (self.yaw + delta_yaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + delta_pitch).clamp(-1.55, 1.55);

        Ok(())
    }

    pub fn bounds(&self) -> Aabb {
        self.bounds_at(self.position)
    }

    fn bounds_at(&self, position: Vec3) -> Aabb {
        Aabb {
            min: position - Vec3::new(self.settings.radius, 0.0, self.settings.radius),
            max: position
                + Vec3::new(
                    self.settings.radius,
                    self.settings.height,
                    self.settings.radius,
                ),
        }
    }

    fn blocked(&self, scene: &Scene, position: Vec3) -> Result<bool> {
        Ok(scene.intersects(self.bounds_at(position))?)
    }

    fn translate(&mut self, scene: &Scene, axis: usize, distance: f32) -> Result<bool> {
        let steps = (distance.abs() / (VOXEL_SIZE * 0.5)).ceil().max(1.0) as usize;

        if steps > self.settings.max_collision_steps {
            return Err(PlayerError::Invalid);
        }

        let delta = distance / steps as f32;

        for _ in 0..steps {
            let mut candidate = self.position;

            candidate[axis] += delta;

            if self.blocked(scene, candidate)? {
                let mut low = 0.0;

                let mut high = 1.0;

                for _ in 0..self.settings.collision_iterations {
                    let mid = (low + high) * 0.5;

                    candidate = self.position;
                    candidate[axis] += delta * mid;

                    if self.blocked(scene, candidate)? {
                        high = mid;
                    } else {
                        low = mid;
                    }
                }

                self.position[axis] += delta * low;

                return Ok(true);
            }

            self.position = candidate;
        }

        Ok(false)
    }

    pub fn advance(&mut self, scene: &Scene, input: PlayerInput, delta: Duration) -> Result<()> {
        if delta > Duration::from_secs(1) || !input.movement.is_finite() {
            return Err(PlayerError::Invalid);
        }

        if delta.is_zero() {
            return Ok(());
        }

        let mut next = self.clone();

        next.step(scene, input, delta.as_secs_f32())?;

        *self = next;

        Ok(())
    }

    fn step(&mut self, scene: &Scene, input: PlayerInput, dt: f32) -> Result<()> {
        let movement = input.movement.clamp_length_max(1.0);

        let forward = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos());

        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());

        let speed = self.settings.speed
            * if input.sprint {
                self.settings.sprint_multiplier
            } else {
                1.0
            };

        if input.jump && self.grounded {
            self.velocity = self.settings.jump_speed;
        }

        let delta = (right * movement.x + forward * movement.y) * speed * dt;

        self.translate(scene, 0, delta.x)?;

        self.translate(scene, 2, delta.z)?;

        self.velocity -= self.settings.gravity * dt;

        let collision = self.translate(scene, 1, self.velocity * dt)?;

        self.grounded = collision && self.velocity <= 0.0;

        if collision {
            self.velocity = 0.0;
        }

        Ok(())
    }
}
