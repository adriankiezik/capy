use glam::Vec2;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 2],
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub position: Vec2,
    pub size: Vec2,
}

impl Rect {
    pub fn new(position: Vec2, size: Vec2) -> Self {
        Self { position, size }
    }

    pub(super) fn intersection(self, other: Self) -> Self {
        let position = self.position.max(other.position);

        Self::new(
            position,
            ((self.position + self.size).min(other.position + other.size) - position)
                .max(Vec2::ZERO),
        )
    }
}
