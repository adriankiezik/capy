use crate::{
    Aabb,
    replica::mesh::{Mesh, Vertex},
};
use glam::Vec3;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Copy, Debug)]
pub struct RenderObject {
    pub id: u64,
    pub bounds: Aabb,
    pub color: [f32; 3],
}

pub(super) struct Object {
    pub(super) description: RenderObject,
    pub(super) mesh: Arc<Mesh>,
}

pub(super) fn build(size: Vec3, color: [f32; 3]) -> Arc<Mesh> {
    let mut vertices = Vec::with_capacity(36);

    for axis in 0..3 {
        let u = (axis + 1) % 3;

        let v = (axis + 2) % 3;

        for sign in [-1, 1] {
            let mut origin = Vec3::ZERO;

            origin[axis] = if sign > 0 { size[axis] } else { 0.0 };

            let mut du = Vec3::ZERO;

            du[u] = size[u];

            let mut dv = Vec3::ZERO;

            dv[v] = size[v];

            let corners = [origin, origin + du, origin + du + dv, origin + dv];

            let mut normal = Vec3::ZERO;

            normal[axis] = sign as f32;

            let order = if sign > 0 {
                [0, 1, 2, 0, 2, 3]
            } else {
                [0, 2, 1, 0, 3, 2]
            };

            for index in order {
                vertices.push(Vertex {
                    position: corners[index].to_array(),
                    normal: normal.to_array(),
                    color,
                    occlusion: 1.0,
                });
            }
        }
    }

    Arc::new(Mesh {
        vertices,
        min: Vec3::ZERO,
        max: size,
        allocation: OnceLock::new(),
    })
}
