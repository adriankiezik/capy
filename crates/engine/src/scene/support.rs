use crate::{
    scene::{Body, Result, SceneError, body::BodyGeometry, connectivity::Connectivity},
    world::{LEAF_EDGE, LEAF_VOXELS, Leaf, LeafCoord, VOXEL_SIZE, Voxel, VoxelCoord, WorldRead},
};
use glam::IVec3;
use std::{collections::BTreeMap, sync::Arc};

pub(super) fn detach(
    cache: &mut Connectivity,
    world: &WorldRead,
    changed: &[VoxelCoord],
    edge: i32,
    max_bodies: usize,
) -> Result<(WorldRead, Vec<Body>, Vec<LeafCoord>)> {
    let groups = cache.unsupported(world, changed)?;

    if groups.len() > max_bodies {
        return Err(SceneError::Limit("dynamic bodies"));
    }

    let mut removal: BTreeMap<LeafCoord, Option<Arc<Leaf>>> = BTreeMap::new();

    let mut bodies = Vec::new();

    for group in groups {
        let leaves = cache.extract(world, &group, |key| world.leaf(key))?;

        let origin = leaves.keys().fold(IVec3::splat(i32::MAX), |min, key| {
            min.min(IVec3::from_array(*key))
        });

        let origin = IVec3::from_array(
            origin
                .to_array()
                .map(|v| (v * LEAF_EDGE).div_euclid(edge) * (edge / LEAF_EDGE)),
        );

        for (&key, selected) in &leaves {
            let remaining = removal
                .entry(key)
                .or_insert_with(|| world.leaf(key).cloned());

            let source = remaining.as_ref().ok_or(SceneError::Invalid)?;

            *remaining = if Arc::ptr_eq(source, selected) {
                None
            } else {
                let voxels: Vec<_> = (0..LEAF_VOXELS)
                    .map(|i| {
                        if selected.voxel(i).is_empty() {
                            source.voxel(i)
                        } else {
                            Voxel::EMPTY
                        }
                    })
                    .collect();

                Leaf::encode(&voxels).map(Arc::new)
            };
        }

        let leaves = leaves
            .into_iter()
            .map(|(key, leaf)| ((IVec3::from_array(key) - origin).to_array(), leaf))
            .collect();

        let geometry = BodyGeometry::new(cache, world, leaves, edge, None)?;

        bodies.push(Body {
            id: 0,
            geometry,
            translation: (origin * LEAF_EDGE).as_vec3() * VOXEL_SIZE,
            velocity: 0.0,
            sleeping: false,
        });
    }

    let removed = removal.keys().copied().collect();

    let world = if removal.is_empty() {
        world.clone()
    } else {
        world.replace_sources(removal)?
    };

    Ok((world, bodies, removed))
}
