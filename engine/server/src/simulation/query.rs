use crate::{
    Aabb,
    simulation::{Result, Simulation, SimulationError},
    world::{LEAF_EDGE, Leaf, LeafCoord, VOXEL_SIZE, Voxel, VoxelCoord, address},
};
use glam::{IVec3, Vec3};

pub use capy_engine_protocol::world::{Hit, Target};

fn trace<'a>(
    origin: Vec3,
    direction: Vec3,
    distance: f32,
    budget: &mut usize,
    source: impl Fn(LeafCoord) -> Option<&'a Leaf>,
) -> Result<Option<(VoxelCoord, Voxel, IVec3, f32)>> {
    let p = origin / VOXEL_SIZE;

    let mut voxel = p.floor().as_ivec3();

    let step = direction.signum().as_ivec3();

    let mut delta = Vec3::splat(f32::INFINITY);

    let mut next = Vec3::splat(f32::INFINITY);

    for axis in 0..3 {
        if direction[axis].abs() > f32::EPSILON {
            delta[axis] = VOXEL_SIZE / direction[axis].abs();

            let boundary = voxel[axis] as f32 + if direction[axis] > 0.0 { 1.0 } else { 0.0 };

            next[axis] = ((boundary - p[axis]) * VOXEL_SIZE / direction[axis]).max(0.0);
        }
    }

    let mut t = 0.0;

    let mut normal = IVec3::ZERO;

    let mut cached = None;

    while t <= distance {
        if *budget == 0 {
            return Err(SimulationError::Limit("query steps"));
        }

        *budget -= 1;

        let (key, index) = address(voxel);

        if cached.as_ref().is_none_or(|(previous, _)| *previous != key) {
            cached = Some((key, source(key)));
        }

        let value = cached
            .and_then(|(_, leaf)| leaf)
            .map_or(Voxel::EMPTY, |leaf| leaf.voxel(index));

        if !value.is_empty() {
            return Ok(Some((voxel, value, normal, t)));
        }

        let axis = if next.x <= next.y && next.x <= next.z {
            0
        } else if next.y <= next.z {
            1
        } else {
            2
        };

        t = next[axis];
        next[axis] += delta[axis];
        voxel[axis] += step[axis];
        normal = IVec3::ZERO;
        normal[axis] = -step[axis];
    }

    Ok(None)
}

impl Simulation {
    pub fn raycast(&self, origin: Vec3, direction: Vec3, distance: f32) -> Result<Option<Hit>> {
        if !origin.is_finite()
            || !direction.is_finite()
            || direction.length_squared() < f32::EPSILON
            || !distance.is_finite()
            || distance <= 0.0
        {
            return Err(SimulationError::Invalid);
        }

        let direction = direction.normalize();

        let mut budget = self.settings.physics.max_collision_probes;

        let bounds = self.world.bounds();

        let mut hit = None;

        if let Some((near, far, entry_normal)) = ray_interval(
            origin,
            direction,
            bounds.min.as_vec3() * VOXEL_SIZE,
            bounds.max.as_vec3() * VOXEL_SIZE,
            distance,
        ) {
            hit = trace(
                origin + direction * near,
                direction,
                far - near,
                &mut budget,
                |key| self.world.leaf(key).map(AsRef::as_ref),
            )?
            .map(|(coordinate, voxel, normal, distance)| Hit {
                target: Target::Static(coordinate),
                voxel,
                normal: if distance == 0.0 {
                    entry_normal
                } else {
                    normal
                },
                distance: distance + near,
            });
        }

        for body in &self.bodies {
            let max_distance = hit.map_or(distance, |hit| hit.distance);

            let origin = origin - body.translation;

            let Some((near, far, entry_normal)) = ray_interval(
                origin,
                direction,
                body.geometry.min,
                body.geometry.max,
                max_distance,
            ) else {
                continue;
            };

            if let Some((coordinate, voxel, normal, distance)) = trace(
                origin + direction * near,
                direction,
                far - near,
                &mut budget,
                |key| body.geometry.leaves.get(&key).map(AsRef::as_ref),
            )? {
                hit = Some(Hit {
                    target: Target::Dynamic {
                        body: body.id,
                        voxel: coordinate,
                    },
                    voxel,
                    normal: if distance == 0.0 {
                        entry_normal
                    } else {
                        normal
                    },
                    distance: distance + near,
                });
            }
        }

        Ok(hit)
    }

    pub fn intersects(&self, bounds: Aabb) -> Result<bool> {
        if !bounds.is_valid() {
            return Err(SimulationError::Invalid);
        }

        let Aabb { min, max } = bounds;

        let mut budget = self.settings.physics.max_collision_probes;

        let bounds = self.world.bounds();

        if min.cmplt(bounds.min.as_vec3() * VOXEL_SIZE).any()
            || max.cmpgt(bounds.max.as_vec3() * VOXEL_SIZE).any()
        {
            return Ok(true);
        }

        if occupied_box(min, max, &mut budget, |key| {
            self.world.leaf(key).map(AsRef::as_ref)
        })? {
            return Ok(true);
        }

        for body in &self.bodies {
            let min = min - body.translation;

            let max = max - body.translation;

            if min.cmplt(body.geometry.max).all()
                && max.cmpgt(body.geometry.min).all()
                && occupied_box(min, max, &mut budget, |key| {
                    body.geometry.leaves.get(&key).map(AsRef::as_ref)
                })?
            {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

fn occupied_box<'a>(
    min: Vec3,
    max: Vec3,
    budget: &mut usize,
    source: impl Fn(LeafCoord) -> Option<&'a Leaf>,
) -> Result<bool> {
    let start = (min / VOXEL_SIZE).floor().as_ivec3();

    let end = (max / VOXEL_SIZE).ceil().as_ivec3();

    for z in start.z..end.z {
        for y in start.y..end.y {
            let mut x = start.x;

            while x < end.x {
                if *budget == 0 {
                    return Err(SimulationError::Limit("collision query"));
                }

                let (key, index) = address(IVec3::new(x, y, z));

                let next = x
                    .saturating_add(LEAF_EDGE - x.rem_euclid(LEAF_EDGE))
                    .min(end.x);

                let count = (next - x) as usize;

                match source(key) {
                    Some(Leaf::Uniform(voxel)) if !voxel.is_empty() => {
                        *budget -= 1;

                        return Ok(true);
                    }
                    None | Some(Leaf::Uniform(_)) => {
                        *budget = budget
                            .checked_sub(count)
                            .ok_or(SimulationError::Limit("collision query"))?;
                    }
                    Some(leaf) => {
                        for offset in 0..count {
                            if *budget == 0 {
                                return Err(SimulationError::Limit("collision query"));
                            }

                            *budget -= 1;

                            if !leaf.voxel(index + offset).is_empty() {
                                return Ok(true);
                            }
                        }
                    }
                }

                x = next;
            }
        }
    }

    Ok(false)
}

fn ray_interval(
    origin: Vec3,
    direction: Vec3,
    min: Vec3,
    max: Vec3,
    distance: f32,
) -> Option<(f32, f32, IVec3)> {
    let mut near = 0.0f32;

    let mut far = distance;

    let mut normal = IVec3::ZERO;

    for axis in 0..3 {
        if direction[axis].abs() < f32::EPSILON {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];

            let b = (max[axis] - origin[axis]) / direction[axis];

            if a.min(b) > near {
                near = a.min(b);
                normal = IVec3::ZERO;
                normal[axis] = if direction[axis] > 0.0 { -1 } else { 1 };
            }

            far = far.min(a.max(b));
        }
    }

    (near <= far).then_some((near, far, normal))
}
