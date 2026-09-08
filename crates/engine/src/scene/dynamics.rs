use crate::{
    scene::{Body, Result, SceneError, SimulationSettings, body::BodyGeometry},
    world::{VOXEL_SIZE, WorldRead},
};
use glam::{IVec3, Vec3};

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
