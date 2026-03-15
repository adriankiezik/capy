use bevy_ecs::resource::Resource;
use glam::Vec3;

#[derive(Resource)]
pub struct VoxelHit {
    pub hit: bool,
    pub position: Vec3,
    pub normal: Vec3,
    pub material: u32,
}

impl Default for VoxelHit {
    fn default() -> Self {
        Self {
            hit: false,
            position: Vec3::ZERO,
            normal: Vec3::ZERO,
            material: 0,
        }
    }
}
