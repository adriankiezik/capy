use crate::{
    scene::{Body, Result, SceneError, dynamics::BodyGeometry, geometry, geometry::MeshSource},
    world::{
        LEAF_EDGE, LEAF_VOXELS, Leaf, StructureId, Support, VOXEL_SIZE, Voxel, VoxelCoord,
        WorldRead, address,
    },
};
use glam::IVec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) fn affected(
    previous: &WorldRead,
    current: &WorldRead,
    changed: &[VoxelCoord],
) -> BTreeSet<StructureId> {
    let mut structures = BTreeSet::new();

    for &p in changed {
        for world in [previous, current] {
            for direction in [IVec3::ZERO, IVec3::Y] {
                let voxel = world.resident_voxel(p + direction);

                if let Some(owner) = world.owner(voxel.owner)
                    && owner.support != Support::Fixed
                {
                    structures.insert(owner.structure);
                }
            }
        }
    }

    structures
}

pub(crate) fn detach(
    world: &WorldRead,
    affected: &BTreeSet<StructureId>,
    next_id: &mut u64,
    edge: i32,
    max_bodies: usize,
) -> Result<(WorldRead, Vec<Body>, Vec<VoxelCoord>)> {
    if affected.is_empty() {
        return Ok((world.clone(), Vec::new(), Vec::new()));
    }

    let limit = world.root.settings.max_support_voxels;

    let mut remaining = BTreeMap::new();

    let mut visited = 0;

    for (p, voxel) in world
        .root
        .owners
        .values()
        .filter(|owner| owner.support != Support::Fixed && affected.contains(&owner.structure))
        .flat_map(|owner| world.owner_voxels(owner.id))
    {
        visited += 1;

        if visited > limit {
            return Err(SceneError::Limit("support scan"));
        }

        remaining.insert(p.to_array(), voxel);
    }

    let mut removal = BTreeMap::new();

    let mut removed = Vec::new();

    let mut bodies = Vec::new();

    while let Some((start, voxel)) = remaining.pop_first() {
        let structure = world
            .owner(voxel.owner)
            .ok_or(SceneError::Invalid)?
            .structure;

        let mut stack = vec![(IVec3::from_array(start), voxel)];

        let mut component = Vec::new();

        let mut supported = false;

        while let Some((p, voxel)) = stack.pop() {
            let owner = world.owner(voxel.owner).ok_or(SceneError::Invalid)?;

            if let Support::Contact(target) = owner.support {
                let below = world.resident_voxel(p - IVec3::Y);

                supported |= below.owner == target;
            }

            component.push((p, voxel));

            for direction in [
                IVec3::X,
                -IVec3::X,
                IVec3::Y,
                -IVec3::Y,
                IVec3::Z,
                -IVec3::Z,
            ] {
                let neighbor = p + direction;

                if let Some(&value) = remaining.get(&neighbor.to_array())
                    && world
                        .owner(value.owner)
                        .is_some_and(|o| o.structure == structure)
                {
                    remaining.remove(&neighbor.to_array());

                    stack.push((neighbor, value));
                }
            }
        }

        if supported {
            continue;
        }

        if bodies.len() >= max_bodies {
            return Err(SceneError::Limit("dynamic bodies"));
        }

        let min = component
            .iter()
            .fold(IVec3::splat(i32::MAX), |p, (c, _)| p.min(*c));

        let max = component
            .iter()
            .fold(IVec3::splat(i32::MIN), |p, (c, _)| p.max(*c + IVec3::ONE));

        let origin = min.to_array().map(|v| v.div_euclid(LEAF_EDGE) * LEAF_EDGE);

        let origin = IVec3::from_array(origin);

        let mut leaves = BTreeMap::new();

        let mut render_keys = BTreeSet::new();

        let mut mass = 0.0;

        for &(p, voxel) in &component {
            let (source_key, source_index) = address(p);

            removal.entry(source_key).or_insert_with(|| {
                world
                    .leaf(source_key)
                    .map_or_else(|| vec![Voxel::EMPTY; LEAF_VOXELS], |leaf| leaf.decode())
            })[source_index] = Voxel::EMPTY;

            removed.push(p);

            let local = p - origin;

            let (key, index) = address(local);

            leaves
                .entry(key)
                .or_insert_with(|| vec![Voxel::EMPTY; LEAF_VOXELS])[index] = voxel;

            render_keys.insert(geometry::key(local, edge));

            mass += world
                .material(voxel.material)
                .ok_or(SceneError::Invalid)?
                .density
                * VOXEL_SIZE.powi(3);
        }

        let leaves = leaves
            .into_iter()
            .filter_map(|(key, voxels)| {
                let source_key = (IVec3::from_array(key) + origin / LEAF_EDGE).to_array();

                let leaf = if let Some(source) = world
                    .leaf(source_key)
                    .filter(|source| (0..LEAF_VOXELS).all(|i| source.voxel(i) == voxels[i]))
                {
                    Some(source.clone())
                } else {
                    Leaf::encode(&voxels).map(Arc::new)
                };

                leaf.map(|leaf| (key, leaf))
            })
            .collect();

        let mut geometry = BodyGeometry {
            leaves,
            meshes: BTreeMap::new(),
            bottom: Vec::new(),
            min: (min - origin).as_vec3() * VOXEL_SIZE,
            max: (max - origin).as_vec3() * VOXEL_SIZE,
            mass,
        };

        geometry.bottom = geometry
            .occupied()
            .filter_map(|(p, _)| geometry.voxel(p - IVec3::Y).is_empty().then_some(p))
            .collect();

        for key in render_keys {
            geometry.meshes.insert(key, Arc::new(MeshSource::default()));
        }

        let id = *next_id;

        *next_id = next_id
            .checked_add(1)
            .ok_or(SceneError::Limit("body identities"))?;

        bodies.push(Body {
            id,
            geometry: Arc::new(geometry),
            translation: origin.as_vec3() * VOXEL_SIZE,
            velocity: 0.0,
            sleeping: false,
        });
    }

    let world = if removed.is_empty() {
        world.clone()
    } else {
        world.replace_leaves(removal)?
    };

    Ok((world, bodies, removed))
}
