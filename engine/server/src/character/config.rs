#[derive(Clone, Debug)]
pub struct CharacterConfig {
    pub radius: f32,
    pub height: f32,
    pub eye_height: f32,
    pub speed: f32,
    pub sprint_multiplier: f32,
    pub jump_speed: f32,
    pub gravity: f32,
    pub max_collision_steps: usize,
    pub collision_iterations: usize,
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            radius: 0.28,
            height: 1.8,
            eye_height: 1.65,
            speed: 4.5,
            sprint_multiplier: 1.7,
            jump_speed: 5.8,
            gravity: 18.0,
            max_collision_steps: 4096,
            collision_iterations: 12,
        }
    }
}
