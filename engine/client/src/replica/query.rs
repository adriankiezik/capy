use super::{
    Hit, Replica, ReplicaError, Result, Target,
    world::{Leaf, VOXEL_SIZE, Voxel, address},
};
use crate::Aabb;
use glam::{IVec3, Vec3};
use std::sync::Arc;

impl Replica {
    pub fn raycast(&self, origin: Vec3, direction: Vec3, distance: f32) -> Result<Option<Hit>> {
        if !origin.is_finite()
            || !direction.is_finite()
            || direction.length_squared() < f32::EPSILON
            || !direction.length_squared().is_finite()
            || !distance.is_finite()
            || distance <= 0.0
        {
            return Err(ReplicaError::Invalid);
        }

        let direction = direction.normalize();

        let mut budget = self.settings.max_query_steps;

        let mut hit = trace(
            origin,
            direction,
            distance,
            Aabb {
                min: self.world.bounds.min.as_vec3() * VOXEL_SIZE,
                max: self.world.bounds.max.as_vec3() * VOXEL_SIZE,
            },
            &self.world.leaves,
            &mut budget,
        )?;

        for body in &self.bodies {
            let Some(bounds) = body.geometry.bounds else {
                continue;
            };

            if let Some(mut next) = trace(
                origin - self.translation(body),
                direction,
                hit.map_or(distance, |hit| hit.distance),
                bounds,
                &body.geometry.leaves,
                &mut budget,
            )? {
                if let Target::Static(voxel) = next.target {
                    next.target = Target::Dynamic {
                        body: body.id,
                        voxel,
                    };
                }

                hit = Some(next);
            }
        }

        Ok(hit)
    }
}

fn trace(
    origin: Vec3,
    direction: Vec3,
    distance: f32,
    bounds: Aabb,
    leaves: &im::OrdMap<[i32; 3], Arc<Leaf>>,
    budget: &mut usize,
) -> Result<Option<Hit>> {
    let mut near = 0.0_f32;

    let mut far = distance;

    let mut normal = IVec3::ZERO;

    for axis in 0..3 {
        if direction[axis].abs() <= f32::EPSILON {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return Ok(None);
            }
        } else {
            let a = (bounds.min[axis] - origin[axis]) / direction[axis];

            let b = (bounds.max[axis] - origin[axis]) / direction[axis];

            if a.min(b) > near {
                near = a.min(b);
                normal = IVec3::ZERO;
                normal[axis] = if direction[axis] > 0.0 { -1 } else { 1 };
            }

            far = far.min(a.max(b));
        }
    }

    if near > far {
        return Ok(None);
    }

    let p = (origin + direction * near) / VOXEL_SIZE;

    let mut coordinate = p.floor().as_ivec3();

    let step = direction.signum().as_ivec3();

    let mut delta = Vec3::splat(f32::INFINITY);

    let mut next = Vec3::splat(f32::INFINITY);

    for axis in 0..3 {
        if direction[axis].abs() > f32::EPSILON {
            delta[axis] = VOXEL_SIZE / direction[axis].abs();

            let boundary = coordinate[axis] as f32 + if direction[axis] > 0.0 { 1.0 } else { 0.0 };

            next[axis] = ((boundary - p[axis]) * VOXEL_SIZE / direction[axis]).max(0.0);
        }
    }

    let mut traveled = 0.0;

    while traveled <= far - near {
        if *budget == 0 {
            return Err(ReplicaError::Limit("query steps"));
        }

        *budget -= 1;

        let (key, index) = address(coordinate);

        let voxel = leaves
            .get(&key)
            .map_or(Voxel::EMPTY, |leaf| leaf.voxel(index));

        if !voxel.is_empty() {
            return Ok(Some(Hit {
                target: Target::Static(coordinate),
                voxel,
                normal,
                distance: near + traveled,
            }));
        }

        let axis = if next.x <= next.y && next.x <= next.z {
            0
        } else if next.y <= next.z {
            1
        } else {
            2
        };

        traveled = next[axis];
        next[axis] += delta[axis];
        coordinate[axis] += step[axis];
        normal = IVec3::ZERO;
        normal[axis] = -step[axis];
    }

    Ok(None)
}
