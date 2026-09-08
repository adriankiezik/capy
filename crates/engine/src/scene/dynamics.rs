use crate::{
    Aabb,
    scene::{Body, Result, SceneError, SimulationSettings, body::BodyGeometry},
    world::{VOXEL_SIZE, WorldRead},
};
use glam::{IVec3, Vec3};

#[derive(Debug, Default)]
pub(super) struct Contacts {
    nodes: Vec<ContactNode>,
    root: Option<usize>,
}

#[derive(Debug)]
struct ContactNode {
    bounds: Aabb,
    entry: ContactEntry,
}

#[derive(Debug)]
enum ContactEntry {
    Body(usize),
    Branch(usize, usize),
}

impl Contacts {
    pub(super) fn new(bodies: &[Body]) -> Self {
        let mut contacts = Self::default();

        let mut bounds: Vec<_> = bodies
            .iter()
            .enumerate()
            .map(|(index, body)| {
                (
                    index,
                    Aabb {
                        min: body.translation + body.geometry.min,
                        max: body.translation + body.geometry.max,
                    },
                )
            })
            .collect();

        if !bounds.is_empty() {
            contacts.root = Some(contacts.build(&mut bounds));
        }

        contacts
    }

    fn build(&mut self, bodies: &mut [(usize, Aabb)]) -> usize {
        let bounds = bodies.iter().fold(
            Aabb {
                min: Vec3::splat(f32::INFINITY),
                max: Vec3::splat(f32::NEG_INFINITY),
            },
            |bounds, (_, body)| Aabb {
                min: bounds.min.min(body.min),
                max: bounds.max.max(body.max),
            },
        );

        let entry = if bodies.len() == 1 {
            ContactEntry::Body(bodies[0].0)
        } else {
            let size = bounds.max - bounds.min;

            let axis = if size.x >= size.y && size.x >= size.z {
                0
            } else if size.y >= size.z {
                1
            } else {
                2
            };

            let middle = bodies.len() / 2;

            bodies.select_nth_unstable_by(middle, |a, b| {
                (a.1.min[axis] + a.1.max[axis]).total_cmp(&(b.1.min[axis] + b.1.max[axis]))
            });

            let (left, right) = bodies.split_at_mut(middle);

            ContactEntry::Branch(self.build(left), self.build(right))
        };

        let index = self.nodes.len();

        self.nodes.push(ContactNode { bounds, entry });

        index
    }

    pub(super) fn wake(&self, bodies: &mut [Body], min: Vec3, max: Vec3, contact_slop: f32) {
        let Some(root) = self.root else { return };

        let slop = Vec3::splat(contact_slop + f32::EPSILON);

        let mut visited = std::collections::HashSet::new();

        let mut pending = vec![Aabb { min, max }];

        let mut nodes = Vec::new();

        while let Some(bounds) = pending.pop() {
            let min = bounds.min - slop;

            let max = bounds.max + slop;

            nodes.push(root);

            while let Some(index) = nodes.pop() {
                let node = &self.nodes[index];

                if !node.bounds.min.cmple(max).all() || !node.bounds.max.cmpge(min).all() {
                    continue;
                }

                match node.entry {
                    ContactEntry::Branch(left, right) => nodes.extend([left, right]),
                    ContactEntry::Body(index) => {
                        if visited.contains(&index) {
                            continue;
                        }

                        let body = &mut bodies[index];

                        let bounds = Aabb {
                            min: body.translation + body.geometry.min,
                            max: body.translation + body.geometry.max,
                        };

                        if bounds.min.cmple(max).all() && bounds.max.cmpge(min).all() {
                            visited.insert(index);

                            body.sleeping = false;

                            pending.push(bounds);
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn step(
    world: &WorldRead,
    bodies: &[Body],
    settings: &SimulationSettings,
    seconds: f32,
) -> Result<Vec<Body>> {
    let mut next = bodies.to_vec();

    next.sort_by(|a, b| {
        (a.translation.y + a.geometry.min.y)
            .total_cmp(&(b.translation.y + b.geometry.min.y))
            .then(a.id.cmp(&b.id))
    });

    let floor_y = world.bounds().min.y as f32 * VOXEL_SIZE;

    let mut probe_count = 0usize;

    let mut candidates = Vec::new();

    let mut regional = Vec::new();

    for index in 0..next.len() {
        if next[index].sleeping {
            continue;
        }

        let body = &next[index];

        let velocity = body.velocity - settings.gravity * seconds;

        let requested_drop = -velocity * seconds;

        let mut allowed_drop = requested_drop.min(
            (body.translation.y + body.geometry.min.y - floor_y - settings.contact_slop).max(0.0),
        );

        let min = body.translation + body.geometry.min
            - Vec3::Y * (requested_drop + settings.contact_slop);

        let max = body.translation + body.geometry.max + Vec3::Y * settings.contact_slop;

        let static_collision = world.overlaps_leaves(
            (min / VOXEL_SIZE).floor().as_ivec3(),
            (max / VOXEL_SIZE).ceil().as_ivec3(),
        );

        candidates.clear();

        candidates.extend(next.iter().enumerate().filter_map(|(other_index, other)| {
            (other_index != index
                && min.cmplt(other.translation + other.geometry.max).all()
                && max.cmpgt(other.translation + other.geometry.min).all())
            .then_some(other_index)
        }));

        for (bounds, points) in body.geometry.bottom_regions() {
            if allowed_drop == 0.0 || !static_collision && candidates.is_empty() {
                break;
            }

            let min =
                body.translation + bounds.min - Vec3::Y * (requested_drop + settings.contact_slop);

            let max = body.translation + bounds.max + Vec3::Y * settings.contact_slop;

            let static_region = static_collision
                && world.overlaps_leaves(
                    (min / VOXEL_SIZE).floor().as_ivec3(),
                    (max / VOXEL_SIZE).ceil().as_ivec3(),
                );

            regional.clear();

            regional.extend(candidates.iter().copied().filter(|&other| {
                let other = &next[other];

                min.cmplt(other.translation + other.geometry.max).all()
                    && max.cmpgt(other.translation + other.geometry.min).all()
            }));

            if !static_region && regional.is_empty() {
                continue;
            }

            for p in points {
                if allowed_drop == 0.0 {
                    break;
                }

                let bottom = p.as_vec3() * VOXEL_SIZE + body.translation;

                if static_region {
                    allowed_drop = allowed_drop.min(world_allowed_drop(
                        world,
                        bottom,
                        requested_drop,
                        settings,
                        &mut probe_count,
                    )?);
                }

                for &other in &regional {
                    allowed_drop = allowed_drop.min(body_allowed_drop(
                        &next[other],
                        bottom,
                        requested_drop,
                        settings,
                        &mut probe_count,
                    )?);
                }
            }
        }

        let body = &mut next[index];

        body.translation.y -= allowed_drop;
        body.sleeping = allowed_drop + f32::EPSILON < requested_drop;
        body.velocity = if body.sleeping { 0.0 } else { velocity };
    }

    next.sort_by_key(|b| b.id);

    Ok(next)
}

struct SweptVoxelColumn {
    x: i32,
    z: i32,
    min_voxel_y: i32,
    max_voxel_y: i32,
}

fn swept_voxel_column(bottom: Vec3, requested_drop: f32, contact_slop: f32) -> SweptVoxelColumn {
    SweptVoxelColumn {
        x: ((bottom.x + VOXEL_SIZE * 0.5) / VOXEL_SIZE).floor() as i32,
        z: ((bottom.z + VOXEL_SIZE * 0.5) / VOXEL_SIZE).floor() as i32,
        min_voxel_y: ((bottom.y - requested_drop - contact_slop) / VOXEL_SIZE).floor() as i32,
        max_voxel_y: ((bottom.y - contact_slop * 0.5) / VOXEL_SIZE).floor() as i32,
    }
}

fn sweep_overlaps_body(
    bottom_in_other: Vec3,
    requested_drop: f32,
    geometry: &BodyGeometry,
    contact_slop: f32,
) -> bool {
    !(bottom_in_other.x + VOXEL_SIZE <= geometry.min.x
        || bottom_in_other.x >= geometry.max.x
        || bottom_in_other.z + VOXEL_SIZE <= geometry.min.z
        || bottom_in_other.z >= geometry.max.z
        || bottom_in_other.y - requested_drop > geometry.max.y + contact_slop
        || bottom_in_other.y < geometry.min.y)
}

fn world_allowed_drop(
    world: &WorldRead,
    bottom: Vec3,
    requested_drop: f32,
    settings: &SimulationSettings,
    probe_count: &mut usize,
) -> Result<f32> {
    let column = swept_voxel_column(bottom, requested_drop, settings.contact_slop);

    let min_voxel_y = column.min_voxel_y.max(world.bounds().min.y);

    let max_voxel_y = column.max_voxel_y.min(world.bounds().max.y - 1);

    for y in (min_voxel_y..=max_voxel_y).rev() {
        *probe_count += 1;

        if *probe_count > settings.max_collision_probes {
            return Err(SceneError::Limit("dynamic collision probes"));
        }

        if !world
            .resident_voxel(IVec3::new(column.x, y, column.z))
            .is_empty()
        {
            return Ok(requested_drop
                .min((bottom.y - (y + 1) as f32 * VOXEL_SIZE - settings.contact_slop).max(0.0)));
        }
    }

    Ok(requested_drop)
}

fn body_allowed_drop(
    other: &Body,
    bottom: Vec3,
    requested_drop: f32,
    settings: &SimulationSettings,
    probe_count: &mut usize,
) -> Result<f32> {
    let bottom_in_other = bottom - other.translation;

    if !sweep_overlaps_body(
        bottom_in_other,
        requested_drop,
        &other.geometry,
        settings.contact_slop,
    ) {
        return Ok(requested_drop);
    }

    let column = swept_voxel_column(bottom_in_other, requested_drop, settings.contact_slop);

    for y in (column.min_voxel_y..=column.max_voxel_y).rev() {
        *probe_count += 1;

        if *probe_count > settings.max_collision_probes {
            return Err(SceneError::Limit("dynamic pair probes"));
        }

        if !other
            .geometry
            .voxel(IVec3::new(column.x, y, column.z))
            .is_empty()
        {
            return Ok(requested_drop.min(
                (bottom_in_other.y - (y + 1) as f32 * VOXEL_SIZE - settings.contact_slop).max(0.0),
            ));
        }
    }

    Ok(requested_drop)
}
