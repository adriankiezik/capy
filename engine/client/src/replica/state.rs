use super::{ReplicaConfig, ReplicaError, Result};
use crate::replica::mesh::{MeshSource, MeshSources, Scheduler};
use crate::replica::world::{Leaf, MaterialId, OwnerId, Voxel, VoxelBounds, WorldRead};
use capy_engine_protocol::message::{BodyState, Chunk, WorldDelta, WorldSnapshot};
use glam::{IVec3, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub(crate) struct BodyGeometry {
    pub(crate) leaves: im::OrdMap<[i32; 3], Arc<Leaf>>,
    pub(crate) meshes: MeshSources,
    pub(crate) bounds: Option<crate::Aabb>,
    revision: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct Body {
    pub(crate) id: u64,
    pub(crate) geometry: Arc<BodyGeometry>,
    pub(crate) translation: Vec3,
    previous: Vec3,
}

pub struct Replica {
    pub(crate) render_cache: std::sync::Mutex<Option<(usize, Arc<[super::MeshInstance]>)>>,
    pub(crate) voxel_instances: Vec<super::VoxelInstance>,
    pub(crate) world: WorldRead,
    pub(crate) meshes: Arc<MeshSources>,
    pub(crate) bodies: Vec<Body>,
    pub(super) objects: BTreeMap<u64, super::object::Object>,
    pub(crate) settings: ReplicaConfig,
    pub(crate) scheduler: Scheduler,
    owners: BTreeSet<OwnerId>,
    revision: u64,
    pub(super) updated: Instant,
}

fn sources(leaves: &im::OrdMap<[i32; 3], Arc<Leaf>>, edge: i32) -> MeshSources {
    leaves
        .keys()
        .map(|&key| {
            (
                crate::replica::mesh::key(IVec3::from_array(key) * 8, edge),
                Arc::new(MeshSource::default()),
            )
        })
        .collect()
}

fn validate_chunks(
    chunks: &[Chunk],
    materials: &BTreeSet<MaterialId>,
    owners: &BTreeSet<OwnerId>,
) -> Result<()> {
    let mut seen = BTreeSet::new();

    if chunks.len() > 65536 {
        return Err(ReplicaError::Limit("replica chunks"));
    }

    for chunk in chunks {
        if !chunk.valid()
            || !seen.insert(chunk.coordinate)
            || chunk.palette.iter().any(|v| {
                *v != Voxel::EMPTY
                    && (!materials.contains(&v.material) || !owners.contains(&v.owner))
            })
        {
            return Err(ReplicaError::Invalid);
        }
    }

    Ok(())
}

fn leaves(chunks: Vec<Chunk>) -> im::OrdMap<[i32; 3], Arc<Leaf>> {
    chunks
        .into_iter()
        .map(|chunk| (chunk.coordinate, Arc::new(Leaf(chunk))))
        .collect()
}

impl Replica {
    pub fn set_objects(&mut self, objects: &[super::RenderObject]) -> Result<()> {
        if objects.len() > 4096 {
            return Err(ReplicaError::Limit("render objects"));
        }

        let mut seen = BTreeSet::new();

        if objects.iter().any(|object| {
            !seen.insert(object.id)
                || !object.bounds.is_valid()
                || object
                    .color
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        }) {
            return Err(ReplicaError::Invalid);
        }

        if self.objects.len() == objects.len()
            && objects.iter().all(|description| {
                self.objects.get(&description.id).is_some_and(|o| {
                    o.description.bounds == description.bounds
                        && o.description.color == description.color
                })
            })
        {
            return Ok(());
        }

        *self
            .render_cache
            .get_mut()
            .unwrap_or_else(|e| e.into_inner()) = None;

        self.objects.retain(|id, _| seen.contains(id));

        for &description in objects {
            let size = description.bounds.max - description.bounds.min;

            if let Some(object) = self.objects.get_mut(&description.id) {
                if (object.description.bounds.max - object.description.bounds.min - size)
                    .abs()
                    .max_element()
                    > 0.0001
                    || object.description.color != description.color
                {
                    object.mesh = super::object::build(size, description.color);
                }

                object.description = description;
            } else {
                self.objects.insert(
                    description.id,
                    super::object::Object {
                        description,
                        mesh: super::object::build(size, description.color),
                    },
                );
            }
        }

        Ok(())
    }

    pub fn empty(settings: ReplicaConfig) -> Result<Self> {
        Self::new(
            settings,
            0,
            WorldSnapshot {
                bounds: VoxelBounds {
                    min: IVec3::ZERO,
                    max: IVec3::ONE,
                },
                materials: Vec::new(),
                owners: Vec::new(),
                chunks: Vec::new(),
                bodies: Vec::new(),
            },
        )
    }

    pub fn new(settings: ReplicaConfig, revision: u64, snapshot: WorldSnapshot) -> Result<Self> {
        settings.visuals.validate()?;

        if settings
            .material_colors
            .values()
            .flatten()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(ReplicaError::Invalid);
        }

        if ![8, 16, 32].contains(&settings.render_leaf_edge)
            || settings.max_mesh_vertices == 0
            || settings.max_query_steps == 0
            || settings.update_interval.is_zero()
            || settings.update_interval > Duration::from_secs(1)
        {
            return Err(ReplicaError::Invalid);
        }

        let VoxelBounds { min, max } = snapshot.bounds;

        if !min.cmplt(max).all()
            || min.cmplt(IVec3::splat(-1_000_000)).any()
            || max.cmpgt(IVec3::splat(1_000_000)).any()
        {
            return Err(ReplicaError::Invalid);
        }

        let material_count = snapshot.materials.len();

        let owner_count = snapshot.owners.len();

        if snapshot.bodies.len() > 4096 || material_count > 65535 || owner_count > 65536 {
            return Err(ReplicaError::Invalid);
        }

        let materials: BTreeMap<_, _> = snapshot.materials.into_iter().map(|m| (m.id, m)).collect();

        if materials.len() != material_count
            || snapshot
                .owners
                .iter()
                .any(|o| o.id.0 == 0 || o.structure.0 == 0)
        {
            return Err(ReplicaError::Invalid);
        }

        if materials.len() > 65535
            || materials
                .values()
                .any(|m| m.id.0 == 0 || !m.density.is_finite() || m.density <= 0.0)
        {
            return Err(ReplicaError::Invalid);
        }

        let owners: BTreeSet<_> = snapshot.owners.into_iter().map(|o| o.id).collect();

        if owners.len() != owner_count {
            return Err(ReplicaError::Invalid);
        }

        validate_chunks(
            &snapshot.chunks,
            &materials.keys().copied().collect(),
            &owners,
        )?;

        if snapshot.chunks.iter().any(|chunk| {
            !snapshot.bounds.intersects(
                (IVec3::from_array(chunk.coordinate) * 8).as_vec3() * 0.1,
                (IVec3::from_array(chunk.coordinate) * 8 + IVec3::splat(8)).as_vec3() * 0.1,
            )
        }) {
            return Err(ReplicaError::Invalid);
        }

        let world = WorldRead {
            leaves: leaves(snapshot.chunks),
            materials: Arc::new(
                materials
                    .keys()
                    .map(|&id| {
                        (
                            id,
                            settings
                                .material_colors
                                .get(&id)
                                .copied()
                                .unwrap_or([0.5; 3]),
                        )
                    })
                    .collect(),
            ),
            bounds: snapshot.bounds,
        };

        let meshes = Arc::new(sources(&world.leaves, settings.render_leaf_edge));

        let mut scene = Self {
            render_cache: Default::default(),
            voxel_instances: Vec::new(),
            world,
            meshes,
            bodies: Vec::new(),
            objects: BTreeMap::new(),
            settings,
            owners,
            revision,
            updated: Instant::now(),
            scheduler: Scheduler::new()?,
        };

        scene.bodies = scene.updated_bodies(snapshot.bodies, &[])?;

        Ok(scene)
    }

    fn updated_bodies(&self, changes: Vec<BodyState>, removed: &[u64]) -> Result<Vec<Body>> {
        let mut bodies: BTreeMap<_, _> = self
            .bodies
            .iter()
            .filter(|b| !removed.contains(&b.id))
            .map(|b| {
                let mut body = b.clone();

                body.previous = self.translation(b);

                (body.id, body)
            })
            .collect();

        let mut seen = BTreeSet::new();

        for change in changes {
            if change.id == 0
                || !seen.insert(change.id)
                || !Vec3::from_array(change.translation).is_finite()
                || !change.velocity.is_finite()
            {
                return Err(ReplicaError::Invalid);
            }

            let translation = Vec3::from_array(change.translation);

            let previous = bodies.get(&change.id).map_or(translation, |b| b.previous);

            let geometry = if let Some(chunks) = change.geometry {
                validate_chunks(
                    &chunks,
                    &self.world.materials.keys().copied().collect(),
                    &self.owners,
                )?;

                let leaves = leaves(chunks);

                let mut meshes = sources(&leaves, self.settings.render_leaf_edge);

                if let Some(previous) = bodies.get(&change.id) {
                    for (&key, source) in &previous.geometry.meshes {
                        if meshes.contains_key(&key) {
                            meshes.insert(key, Arc::new(MeshSource::replacement(source)));
                        }
                    }
                }

                let bounds = leaves.keys().fold(None::<crate::Aabb>, |bounds, &key| {
                    let min =
                        (IVec3::from_array(key) * 8).as_vec3() * crate::replica::world::VOXEL_SIZE;

                    let max = min + Vec3::splat(8.0 * crate::replica::world::VOXEL_SIZE);

                    Some(
                        bounds.map_or(crate::Aabb { min, max }, |bounds| crate::Aabb {
                            min: bounds.min.min(min),
                            max: bounds.max.max(max),
                        }),
                    )
                });

                Arc::new(BodyGeometry {
                    leaves,
                    meshes,
                    bounds,
                    revision: change.geometry_revision,
                })
            } else {
                bodies
                    .get(&change.id)
                    .filter(|b| b.geometry.revision == change.geometry_revision)
                    .ok_or(ReplicaError::Invalid)?
                    .geometry
                    .clone()
            };

            bodies.insert(
                change.id,
                Body {
                    id: change.id,
                    geometry,
                    translation,
                    previous,
                },
            );
        }

        if bodies.len() > 4096
            || bodies
                .values()
                .map(|b| b.geometry.leaves.len())
                .sum::<usize>()
                > 65536
        {
            return Err(ReplicaError::Limit("replica bodies"));
        }

        Ok(bodies.into_values().collect())
    }

    pub fn apply(&mut self, base: u64, revision: u64, delta: WorldDelta) -> Result<()> {
        if delta.removed_chunks.len() > 65536
            || delta.removed_bodies.len() > 4096
            || delta.bodies.len() > 4096
            || delta
                .removed_chunks
                .iter()
                .flatten()
                .any(|v| !(-125_001..=125_001).contains(v))
        {
            return Err(ReplicaError::Invalid);
        }

        if base != self.revision || revision <= base {
            return Err(ReplicaError::RevisionMismatch);
        }

        validate_chunks(
            &delta.chunks,
            &self.world.materials.keys().copied().collect(),
            &self.owners,
        )?;

        if delta.chunks.iter().any(|chunk| {
            !self.world.bounds.intersects(
                (IVec3::from_array(chunk.coordinate) * 8).as_vec3() * 0.1,
                (IVec3::from_array(chunk.coordinate) * 8 + IVec3::splat(8)).as_vec3() * 0.1,
            )
        }) {
            return Err(ReplicaError::Invalid);
        }

        let bodies = self.updated_bodies(delta.bodies, &delta.removed_bodies)?;

        let mut next = self.world.clone();

        let mut dirty = BTreeSet::new();

        for key in delta
            .removed_chunks
            .iter()
            .copied()
            .chain(delta.chunks.iter().map(|c| c.coordinate))
        {
            let min = IVec3::from_array(crate::replica::mesh::key(
                IVec3::from_array(key) * 8 - IVec3::ONE,
                self.settings.render_leaf_edge,
            ));

            let max = IVec3::from_array(crate::replica::mesh::key(
                IVec3::from_array(key) * 8 + IVec3::splat(8),
                self.settings.render_leaf_edge,
            ));

            for z in min.z..=max.z {
                for y in min.y..=max.y {
                    for x in min.x..=max.x {
                        dirty.insert([x, y, z]);
                    }
                }
            }
        }

        for key in delta.removed_chunks {
            next.leaves.remove(&key);
        }

        for chunk in delta.chunks {
            next.leaves.insert(chunk.coordinate, Arc::new(Leaf(chunk)));
        }

        if next.leaves.len() > 65536 {
            return Err(ReplicaError::Limit("replica chunks"));
        }

        let keys: BTreeSet<_> = next
            .leaves
            .keys()
            .map(|&key| {
                crate::replica::mesh::key(
                    IVec3::from_array(key) * 8,
                    self.settings.render_leaf_edge,
                )
            })
            .collect();

        let mut meshes = (*self.meshes).clone();

        for key in dirty {
            if keys.contains(&key) {
                let source = meshes.get(&key).map_or_else(MeshSource::default, |source| {
                    MeshSource::replacement(source)
                });

                meshes.insert(key, Arc::new(source));
            } else {
                meshes.remove(&key);
            }
        }

        *self
            .render_cache
            .get_mut()
            .unwrap_or_else(|e| e.into_inner()) = None;
        self.world = next;
        self.meshes = Arc::new(meshes);
        self.bodies = bodies;
        self.revision = revision;
        self.updated = Instant::now();

        Ok(())
    }

    pub(crate) fn translation(&self, body: &Body) -> Vec3 {
        body.previous.lerp(
            body.translation,
            (self.updated.elapsed().as_secs_f32() / self.settings.update_interval.as_secs_f32())
                .min(1.0),
        )
    }
}
