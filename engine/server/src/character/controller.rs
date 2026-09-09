use std::time::Duration;

use crate::{
    Aabb,
    character::{CharacterConfig, CharacterError, Result},
    simulation::Simulation,
    world::VOXEL_SIZE,
};
use glam::{Vec2, Vec3};

#[derive(Clone, Copy, Debug, Default)]
pub struct MovementInput {
    pub movement: Vec2,
    pub sprint: bool,
    pub jump: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterPose {
    pub position: Vec3,
    pub yaw_radians: f32,
    pub pitch_radians: f32,
}

#[derive(Clone, Debug)]
pub struct Character {
    settings: CharacterConfig,
    position: Vec3,
    yaw: f32,
    pitch: f32,
    velocity: f32,
    grounded: bool,
}

impl Character {
    pub fn new(settings: CharacterConfig, pose: CharacterPose) -> Result<Self> {
        let CharacterPose {
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
            ]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
            || settings.eye_height > settings.height
            || settings.max_collision_steps == 0
            || !(1..=24).contains(&settings.collision_iterations)
        {
            return Err(CharacterError::Invalid);
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

    pub fn pose(&self) -> CharacterPose {
        CharacterPose {
            position: self.position,
            yaw_radians: self.yaw,
            pitch_radians: self.pitch,
        }
    }

    pub fn eye(&self) -> Vec3 {
        self.position + Vec3::Y * self.settings.eye_height
    }

    pub fn direction(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    pub fn rotate_view(&mut self, delta_yaw: f32, delta_pitch: f32) -> Result<()> {
        if !delta_yaw.is_finite() || !delta_pitch.is_finite() {
            return Err(CharacterError::Invalid);
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

    fn blocked(&self, scene: &Simulation, position: Vec3) -> Result<bool> {
        Ok(scene.intersects(self.bounds_at(position))?)
    }

    fn translate(&mut self, scene: &Simulation, axis: usize, distance: f32) -> Result<bool> {
        let steps = (distance.abs() / (VOXEL_SIZE * 0.5)).ceil().max(1.0) as usize;

        if steps > self.settings.max_collision_steps {
            return Err(CharacterError::Invalid);
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

    pub fn advance(
        &mut self,
        scene: &Simulation,
        input: MovementInput,
        delta: Duration,
    ) -> Result<()> {
        if delta > Duration::from_secs(1) || !input.movement.is_finite() {
            return Err(CharacterError::Invalid);
        }

        if delta.is_zero() {
            return Ok(());
        }

        let mut next = self.clone();

        next.step(scene, input, delta.as_secs_f32())?;

        *self = next;

        Ok(())
    }

    fn step(&mut self, scene: &Simulation, input: MovementInput, dt: f32) -> Result<()> {
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
