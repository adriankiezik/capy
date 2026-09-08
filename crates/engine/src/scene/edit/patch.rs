use super::{Domain, Limits, local::LocalPatch};
use crate::{
    scene::{
        Body, Result, Scene, SceneError,
        body::BodyGeometry,
        connectivity::Connectivity,
        dynamics,
        geometry::{self, MeshSource, MeshSources},
        support,
        work::Work,
    },
    world::{
        LEAF_VOXELS, Leaf, LeafCoord, Transaction, VoxelCoord, WorldRead, coordinate, prepare,
    },
};
use glam::{IVec3, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(in crate::scene) struct StaticPatch {
    base: WorldRead,
    world: WorldRead,
    meshes: Arc<MeshSources>,
    additions: Vec<Body>,
    dirty: BTreeSet<[i32; 3]>,
    dependencies: BTreeSet<LeafCoord>,
    changes: BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
}

struct StaticDraft {
    base: WorldRead,
    world: WorldRead,
    meshes: Arc<MeshSources>,
    additions: Vec<Body>,
    dirty: BTreeSet<[i32; 3]>,
}

impl StaticPatch {
    pub(in crate::scene) fn prepare(
        base: &WorldRead,
        sources: &Arc<MeshSources>,
        cache: &mut Connectivity,
        transaction: Transaction,
        limits: Limits,
    ) -> Result<Self> {
        pollster::block_on(async {
            StaticDraft::prepare(base, sources, cache, transaction, limits)
                .await?
                .finish(None)
                .await
        })
    }
}

impl StaticDraft {
    async fn prepare(
        base: &WorldRead,
        sources: &Arc<MeshSources>,
        cache: &mut Connectivity,
        transaction: Transaction,
        limits: Limits,
    ) -> Result<Self> {
        let tracked = base.tracked();

        let base = &tracked;

        let (world, changed) = prepare(base, transaction)?;

        if changed.is_empty() {
            return Ok(Self {
                base: base.clone(),
                world,
                meshes: sources.clone(),
                additions: Vec::new(),
                dirty: BTreeSet::new(),
            });
        }

        let (world, mut additions, removed) =
            support::detach(cache, &world, &changed, limits.edge, limits.bodies).await?;

        for body in &mut additions {
            Arc::get_mut(&mut body.geometry)
                .ok_or(SceneError::Invalid)?
                .inherit_meshes(base, sources, body.translation, limits.edge)
                .await;
        }

        let mut dirty = BTreeSet::new();

        for voxel in changed {
            dirty_keys(&mut dirty, voxel, voxel, limits.edge);
        }

        for leaf in removed {
            dirty_keys(
                &mut dirty,
                coordinate(leaf, 0),
                coordinate(leaf, LEAF_VOXELS - 1),
                limits.edge,
            );
        }

        let mut meshes = (**sources).clone();

        for &key in &dirty {
            if world.has_render_voxels(key, limits.edge) {
                meshes.insert(key, Arc::new(MeshSource::default()));
            } else {
                meshes.remove(&key);
            }
        }

        Ok(Self {
            base: base.clone(),
            world,
            meshes: Arc::new(meshes),
            additions,
            dirty,
        })
    }

    async fn warm(&self, limits: Limits) -> Result<()> {
        let mut available = limits.vertices;

        let mut work = Work::default();

        for key in &self.dirty {
            work.checkpoint().await;

            if let Some(source) = self.meshes.get(key) {
                let mesh = source
                    .resolve_async(
                        *key,
                        limits.edge,
                        |key| self.world.leaf(key).map(Arc::as_ref),
                        &self.world,
                        available,
                    )
                    .await?;

                available -= mesh.vertices.len();
            }
        }

        for body in &self.additions {
            body.geometry
                .prepare_meshes(&self.world, limits.edge, &mut available)
                .await?;
        }

        Ok(())
    }

    async fn finish(mut self, warm: Option<Limits>) -> Result<StaticPatch> {
        if let Some(limits) = warm {
            self.warm(limits).await?;
        }

        let dependencies = self.base.finish_reads();

        let world = self.world.untracked();

        let mut changes = BTreeMap::new();

        let mut work = Work::default();

        for &key in &dependencies {
            work.checkpoint().await;

            if !same_leaf(self.base.leaf(key), world.leaf(key)) {
                changes.insert(key, world.leaf(key).cloned());
            }
        }

        Ok(StaticPatch {
            base: self.base,
            world,
            meshes: self.meshes,
            additions: self.additions,
            dirty: self.dirty,
            dependencies,
            changes,
        })
    }
}

impl StaticPatch {
    pub(in crate::scene) fn publish(&mut self, scene: &mut Scene) -> Result<Vec<Body>> {
        if !Arc::ptr_eq(&self.base.root, &scene.world.root)
            && self
                .dependencies
                .iter()
                .any(|&key| !same_leaf(self.base.leaf(key), scene.world.leaf(key)))
        {
            return Err(SceneError::StaleEdit);
        }

        if Arc::ptr_eq(&self.base.root, &self.world.root) {
            return Ok(Vec::new());
        }

        let next = reserve(scene, 0, self.additions.len(), self.additions.len())?;

        for (offset, body) in self.additions.iter_mut().enumerate() {
            body.id = scene.next_body + offset as u64;
        }

        if Arc::ptr_eq(&self.base.root, &scene.world.root) {
            std::mem::swap(&mut scene.world, &mut self.world);

            std::mem::swap(&mut scene.meshes, &mut self.meshes);
        } else {
            let world = scene.world.replace_sources(self.changes.clone())?;

            self.world = std::mem::replace(&mut scene.world, world);

            let mut meshes = (*scene.meshes).clone();

            for key in &self.dirty {
                if let Some(source) = self.meshes.get(key) {
                    meshes.insert(*key, source.clone());
                } else {
                    meshes.remove(key);
                }
            }

            self.meshes = std::mem::replace(&mut scene.meshes, Arc::new(meshes));
        }

        scene.bodies.extend(self.additions.iter().cloned());

        scene.next_body = next;

        if !self.additions.is_empty() {
            scene.contacts = Arc::default();
        }

        let min = self
            .changes
            .keys()
            .fold(IVec3::splat(i32::MAX), |min, &key| {
                min.min(coordinate(key, 0))
            });

        let max = self
            .changes
            .keys()
            .fold(IVec3::splat(i32::MIN), |max, &key| {
                max.max(coordinate(key, LEAF_VOXELS - 1) + IVec3::ONE)
            });

        wake_bounds(
            scene,
            min.as_vec3() * crate::world::VOXEL_SIZE,
            max.as_vec3() * crate::world::VOXEL_SIZE,
        );

        Ok(self.additions.clone())
    }
}

pub(super) enum Input {
    Static {
        world: WorldRead,
        meshes: Arc<MeshSources>,
        limits: Limits,
    },
    Dynamic {
        world: WorldRead,
        id: u64,
        geometry: Arc<BodyGeometry>,
        limits: Limits,
    },
}

impl Input {
    pub(super) fn capture(scene: &Scene, domain: Domain) -> Option<Self> {
        let world = scene.world.clone();

        let limits = Limits::new(scene);

        Some(match domain {
            Domain::Static => Self::Static {
                world,
                meshes: scene.meshes.clone(),
                limits,
            },
            Domain::Body(id) => Self::Dynamic {
                world,
                id,
                geometry: scene.bodies[scene
                    .bodies
                    .binary_search_by_key(&id, |body| body.id)
                    .ok()?]
                .geometry
                .clone(),
                limits,
            },
        })
    }

    pub(super) async fn prepare(
        self,
        voxels: Vec<VoxelCoord>,
        cache: &mut Connectivity,
    ) -> Result<Patch> {
        match self {
            Self::Static {
                world,
                meshes,
                limits,
            } => {
                let mut transaction = Transaction::new(&world);

                for voxel in voxels {
                    transaction.remove(voxel)?;
                }

                let patch = StaticDraft::prepare(&world, &meshes, cache, transaction, limits)
                    .await?
                    .finish(Some(limits))
                    .await?;

                Ok(Patch::Static(patch))
            }
            Self::Dynamic {
                world,
                id,
                geometry,
                limits,
            } => {
                let voxels: Vec<_> = voxels
                    .into_iter()
                    .filter(|&voxel| !geometry.voxel(voxel).is_empty())
                    .collect();

                if voxels.is_empty() {
                    return Ok(Patch::Dynamic {
                        id,
                        base: geometry.clone(),
                        replacements: vec![geometry],
                    });
                }

                let replacements = geometry
                    .remove_async(cache, &world, &voxels, limits.edge, limits.bodies)
                    .await?;

                let mut available = limits.vertices;

                for geometry in &replacements {
                    geometry
                        .prepare_meshes(&world, limits.edge, &mut available)
                        .await?;
                }

                Ok(Patch::Dynamic {
                    id,
                    base: geometry,
                    replacements,
                })
            }
        }
    }
}

pub(in crate::scene) enum Patch {
    Local(Box<LocalPatch>),
    StructuralRequired,
    Unchanged,
    Static(StaticPatch),
    Dynamic {
        id: u64,
        base: Arc<BodyGeometry>,
        replacements: Vec<Arc<BodyGeometry>>,
    },
}

impl Patch {
    pub(in crate::scene) fn publish(&mut self, scene: &mut Scene) -> Result<Vec<Body>> {
        match self {
            Self::Local(patch) => patch.publish(scene),
            Self::StructuralRequired => Err(SceneError::Invalid),
            Self::Unchanged => Ok(Vec::new()),
            Self::Static(patch) => patch.publish(scene),
            Self::Dynamic {
                id,
                base,
                replacements,
            } => {
                let index = scene
                    .bodies
                    .binary_search_by_key(id, |body| body.id)
                    .map_err(|_| SceneError::StaleEdit)?;

                let current = &scene.bodies[index];

                if !Arc::ptr_eq(base, &current.geometry) {
                    return Err(SceneError::StaleEdit);
                }

                if replacements.len() == 1 && Arc::ptr_eq(base, &replacements[0]) {
                    return Ok(vec![current.clone()]);
                }

                let next = reserve(
                    scene,
                    1,
                    replacements.len(),
                    replacements.len().saturating_sub(1),
                )?;

                let min = current.translation + current.geometry.min;

                let max = current.translation + current.geometry.max;

                let bodies: Vec<_> = replacements
                    .iter()
                    .enumerate()
                    .map(|(offset, geometry)| Body {
                        id: if offset == 0 {
                            *id
                        } else {
                            scene.next_body + offset as u64 - 1
                        },
                        geometry: geometry.clone(),
                        translation: current.translation,
                        velocity: current.velocity,
                        sleeping: false,
                    })
                    .collect();

                scene.bodies.splice(index..=index, bodies.iter().cloned());

                scene.bodies.sort_by_key(|body| body.id);

                scene.next_body = next;
                scene.contacts = Arc::default();

                wake_bounds(scene, min, max);

                Ok(bodies)
            }
        }
    }
}

pub(super) fn same_leaf(a: Option<&Arc<Leaf>>, b: Option<&Arc<Leaf>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn reserve(scene: &Scene, removed: usize, added: usize, identities: usize) -> Result<u64> {
    if added
        > scene
            .settings
            .max_bodies
            .saturating_sub(scene.bodies.len() - removed)
    {
        return Err(SceneError::Limit("dynamic bodies"));
    }

    scene
        .next_body
        .checked_add(identities as u64)
        .ok_or(SceneError::Limit("body identities"))
}

pub(super) fn wake_bounds(scene: &mut Scene, min: Vec3, max: Vec3) {
    let contacts = scene
        .contacts
        .get_or_init(|| dynamics::Contacts::new(&scene.bodies));

    contacts.wake(
        &mut scene.bodies,
        min,
        max,
        scene.settings.simulation.contact_slop,
    );
}

pub(super) fn dirty_keys(
    dirty: &mut BTreeSet<[i32; 3]>,
    min: VoxelCoord,
    max: VoxelCoord,
    edge: i32,
) {
    let min = IVec3::from_array(geometry::key(min - IVec3::ONE, edge));

    let max = IVec3::from_array(geometry::key(max + IVec3::ONE, edge));

    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                dirty.insert([x, y, z]);
            }
        }
    }
}
