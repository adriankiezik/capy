use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn is_valid(self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min.cmplt(self.max).all()
    }

    pub fn intersects(self, other: Self) -> bool {
        self.is_valid()
            && other.is_valid()
            && self.min.cmplt(other.max).all()
            && self.max.cmpgt(other.min).all()
    }
}
