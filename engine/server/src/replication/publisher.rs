use crate::{
    simulation::Simulation,
    world::{LEAF_VOXELS, Leaf},
};
use capy_engine_protocol::message::{BodyState, Chunk, WorldDelta, WorldSnapshot};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) struct Publisher {
    previous: Option<Simulation>,
    versions: BTreeMap<u64, u64>,
}

fn chunk(coordinate: [i32; 3], leaf: &Leaf) -> Chunk {
    let mut palette = Vec::new();

    let mut indices = Vec::with_capacity(LEAF_VOXELS);

    let mut lookup = BTreeMap::new();

    for i in 0..LEAF_VOXELS {
        let voxel = leaf.voxel(i);

        let index = *lookup.entry(voxel).or_insert_with(|| {
            let index = palette.len() as u16;

            palette.push(voxel);

            index
        });

        indices.push(index);
    }

    if palette.len() == 1 {
        indices.clear();
    }

    Chunk {
        coordinate,
        palette,
        indices,
    }
}

impl Publisher {
    pub(crate) fn new() -> Self {
        Self {
            previous: None,
            versions: BTreeMap::new(),
        }
    }

    pub(crate) fn update(&mut self, scene: &Simulation, revision: u64) -> WorldDelta {
        let mut delta = WorldDelta {
            chunks: Vec::new(),
            removed_chunks: Vec::new(),
            bodies: Vec::new(),
            removed_bodies: Vec::new(),
        };

        for region in scene.world.root.regions.values() {
            for (&key, leaf) in region.iter() {
                if self
                    .previous
                    .as_ref()
                    .and_then(|s| s.world.leaf(key))
                    .is_none_or(|old| !Arc::ptr_eq(old, leaf))
                {
                    delta.chunks.push(chunk(key, leaf));
                }
            }
        }

        if let Some(previous) = &self.previous {
            for region in previous.world.root.regions.values() {
                for &key in region.keys() {
                    if scene.world.leaf(key).is_none() {
                        delta.removed_chunks.push(key);
                    }
                }
            }

            for body in &previous.bodies {
                if scene
                    .bodies
                    .binary_search_by_key(&body.id, |b| b.id)
                    .is_err()
                {
                    delta.removed_bodies.push(body.id);

                    self.versions.remove(&body.id);
                }
            }
        }

        for body in &scene.bodies {
            let old = self.previous.as_ref().and_then(|s| {
                s.bodies
                    .binary_search_by_key(&body.id, |b| b.id)
                    .ok()
                    .map(|i| &s.bodies[i])
            });

            let changed = old.is_none_or(|old| !Arc::ptr_eq(&old.geometry, &body.geometry));

            if changed {
                self.versions.insert(body.id, revision);
            }

            if changed
                || old.is_none_or(|old| {
                    old.translation != body.translation
                        || old.velocity != body.velocity
                        || old.sleeping != body.sleeping
                })
            {
                delta.bodies.push(BodyState {
                    id: body.id,
                    translation: body.translation.to_array(),
                    velocity: body.velocity,
                    sleeping: body.sleeping,
                    geometry_revision: self.versions[&body.id],
                    geometry: changed.then(|| {
                        body.geometry
                            .leaves
                            .iter()
                            .map(|(&key, leaf)| chunk(key, leaf))
                            .collect()
                    }),
                });
            }
        }

        self.previous = Some(scene.clone());

        delta
    }

    pub(crate) fn snapshot(&self, scene: &Simulation) -> WorldSnapshot {
        WorldSnapshot {
            bounds: scene.world.bounds(),
            materials: scene.world.root.settings.materials.clone(),
            owners: scene.world.root.settings.owners.clone(),
            chunks: scene
                .world
                .root
                .regions
                .values()
                .flat_map(|region| region.iter().map(|(&key, leaf)| chunk(key, leaf)))
                .collect(),
            bodies: scene
                .bodies
                .iter()
                .map(|body| BodyState {
                    id: body.id,
                    translation: body.translation.to_array(),
                    velocity: body.velocity,
                    sleeping: body.sleeping,
                    geometry_revision: self.versions.get(&body.id).copied().unwrap_or(0),
                    geometry: Some(
                        body.geometry
                            .leaves
                            .iter()
                            .map(|(&key, leaf)| chunk(key, leaf))
                            .collect(),
                    ),
                })
                .collect(),
        }
    }
}
