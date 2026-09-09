use crate::Aabb;
use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy)]
pub(super) enum Order {
    Perspective(Vec3),
    Directional(Vec3),
}

impl Order {
    fn distance(self, bounds: Aabb) -> f32 {
        match self {
            Self::Perspective(eye) => eye.distance_squared(eye.clamp(bounds.min, bounds.max)),
            Self::Directional(direction) => -direction.dot((bounds.min + bounds.max) * 0.5),
        }
    }
}

pub(super) struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub(super) fn new(matrix: Mat4) -> Self {
        let rows = matrix.transpose().to_cols_array_2d().map(Vec4::from_array);

        Self {
            planes: [
                rows[3] + rows[0],
                rows[3] - rows[0],
                rows[3] + rows[1],
                rows[3] - rows[1],
                rows[2],
                rows[3] - rows[2],
            ],
        }
    }

    fn intersects(&self, bounds: Aabb) -> bool {
        self.planes.iter().all(|plane| {
            let positive = Vec3::select(plane.truncate().cmpge(Vec3::ZERO), bounds.max, bounds.min);

            plane.truncate().dot(positive) + plane.w >= 0.0
        })
    }
}

struct Node {
    bounds: Aabb,
    children: Option<[usize; 2]>,
    leaf: usize,
}

#[derive(Default)]
pub(super) struct Hierarchy {
    nodes: Vec<Node>,
}

impl Hierarchy {
    pub(super) fn bounds(&self) -> Option<Aabb> {
        self.nodes.first().map(|node| node.bounds)
    }

    pub(super) fn rebuild(&mut self, bounds: &[Aabb]) {
        self.nodes.clear();

        let mut indices: Vec<_> = (0..bounds.len()).collect();

        if !indices.is_empty() {
            self.build(&mut indices, bounds);
        }
    }

    fn build(&mut self, indices: &mut [usize], bounds: &[Aabb]) -> usize {
        let index = self.nodes.len();

        let combined = indices.iter().fold(
            Aabb {
                min: Vec3::splat(f32::INFINITY),
                max: Vec3::splat(f32::NEG_INFINITY),
            },
            |a, &i| Aabb {
                min: a.min.min(bounds[i].min),
                max: a.max.max(bounds[i].max),
            },
        );

        self.nodes.push(Node {
            bounds: combined,
            children: None,
            leaf: indices[0],
        });

        if indices.len() > 1 {
            let extent = combined.max - combined.min;

            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };

            let middle = indices.len() / 2;

            indices.select_nth_unstable_by(middle, |&a, &b| {
                (bounds[a].min[axis] + bounds[a].max[axis])
                    .total_cmp(&(bounds[b].min[axis] + bounds[b].max[axis]))
                    .then(a.cmp(&b))
            });

            let (left, right) = indices.split_at_mut(middle);

            let children = [self.build(left, bounds), self.build(right, bounds)];

            self.nodes[index].children = Some(children);
        }

        index
    }

    pub(super) fn refit(&mut self, bounds: &[Aabb]) {
        for i in (0..self.nodes.len()).rev() {
            self.nodes[i].bounds = if let Some([left, right]) = self.nodes[i].children {
                Aabb {
                    min: self.nodes[left]
                        .bounds
                        .min
                        .min(self.nodes[right].bounds.min),
                    max: self.nodes[left]
                        .bounds
                        .max
                        .max(self.nodes[right].bounds.max),
                }
            } else {
                bounds[self.nodes[i].leaf]
            };
        }
    }

    pub(super) fn visible(&self, matrix: Mat4, order: Order, output: &mut Vec<usize>) {
        output.clear();

        if !self.nodes.is_empty() {
            self.visit(0, &Frustum::new(matrix), order, output);
        }
    }

    fn visit(&self, index: usize, frustum: &Frustum, order: Order, output: &mut Vec<usize>) {
        let node = &self.nodes[index];

        if !frustum.intersects(node.bounds) {
            return;
        }

        if let Some([mut left, mut right]) = node.children {
            if order.distance(self.nodes[left].bounds) > order.distance(self.nodes[right].bounds) {
                std::mem::swap(&mut left, &mut right);
            }

            self.visit(left, frustum, order, output);

            self.visit(right, frustum, order, output);
        } else {
            output.push(node.leaf);
        }
    }
}
