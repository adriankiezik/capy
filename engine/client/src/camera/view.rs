use glam::{Mat4, Vec3};

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
