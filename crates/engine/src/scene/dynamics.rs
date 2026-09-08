use crate::{
    scene::{Result, SceneError, SimulationSettings, geometry::MeshSource},
    world::{
        LEAF_VOXELS, Leaf, LeafCoord, VOXEL_SIZE, Voxel, VoxelCoord, WorldRead, address, coordinate,
    },
};
use glam::{IVec3, Vec3};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug)]
pub(crate) struct BodyGeometry {
    pub(crate) leaves: BTreeMap<LeafCoord, Arc<Leaf>>,
    pub(crate) meshes: BTreeMap<[i32; 3], Arc<MeshSource>>,
    pub(crate) bottom: Vec<VoxelCoord>,
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
    pub(crate) mass: f32,
}

impl BodyGeometry {
    pub(crate) fn voxel(&self, p: VoxelCoord) -> Voxel {
        let (key, index) = address(p);

        self.leaves
            .get(&key)
            .map_or(Voxel::EMPTY, |leaf| leaf.voxel(index))
    }

    pub(crate) fn occupied(&self) -> impl Iterator<Item = (VoxelCoord, Voxel)> + '_ {
        self.leaves.iter().flat_map(|(&key, leaf)| {
            (0..LEAF_VOXELS).filter_map(move |i| {
                let voxel = leaf.voxel(i);

                (!voxel.is_empty()).then_some((coordinate(key, i), voxel))
            })
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Body {
    pub(crate) id: u64,
    pub(crate) geometry: Arc<BodyGeometry>,
    pub(crate) translation: Vec3,
    pub(crate) velocity: f32,
    pub(crate) sleeping: bool,
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

        for p in &body.geometry.bottom {
            let bottom = p.as_vec3() * VOXEL_SIZE + body.translation;

            allowed_drop = allowed_drop.min(world_allowed_drop(
                world,
                bottom,
                requested_drop,
                settings,
                &mut probe_count,
            )?);

            for (other_index, other) in next.iter().enumerate() {
                if other_index == index {
                    continue;
                }

                allowed_drop = allowed_drop.min(body_allowed_drop(
                    other,
                    bottom,
                    requested_drop,
                    settings,
                    &mut probe_count,
                )?);
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
