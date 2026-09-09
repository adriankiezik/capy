use super::{Domain, Limits, local::LocalPatch};
use crate::{
    simulation::{
        Body, Result, Simulation, SimulationError, body::BodyGeometry, connectivity::Connectivity,
        dynamics, support, work::Work,
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

pub(in crate::simulation) struct StaticPatch {
    base: WorldRead,
    world: WorldRead,
    additions: Vec<Body>,
    dependencies: BTreeSet<LeafCoord>,
    changes: BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
}

struct StaticDraft {
    base: WorldRead,
    world: WorldRead,
    additions: Vec<Body>,
}

impl StaticPatch {
    pub(in crate::simulation) fn prepare(
        base: &WorldRead,
        cache: &mut Connectivity,
        transaction: Transaction,
        limits: Limits,
    ) -> Result<Self> {
        pollster::block_on(async {
            StaticDraft::prepare(base, cache, transaction, limits)
                .await?
                .finish()
                .await
        })
    }
}

impl StaticDraft {
    async fn prepare(
        base: &WorldRead,
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
                additions: Vec::new(),
            });
        }

        let (world, additions, _) = support::detach(cache, &world, &changed, limits.bodies).await?;

        Ok(Self {
            base: base.clone(),
            world,
            additions,
        })
    }

    async fn finish(mut self) -> Result<StaticPatch> {
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
            additions: self.additions,
            dependencies,
            changes,
        })
    }
}

impl StaticPatch {
    pub(in crate::simulation) fn publish(&mut self, scene: &mut Simulation) -> Result<Vec<Body>> {
        if !Arc::ptr_eq(&self.base.root, &scene.world.root)
            && self
                .dependencies
                .iter()
                .any(|&key| !same_leaf(self.base.leaf(key), scene.world.leaf(key)))
        {
            return Err(SimulationError::StaleEdit);
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
        } else {
            let world = scene.world.replace_sources(self.changes.clone())?;

            self.world = std::mem::replace(&mut scene.world, world);
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
    pub(super) fn capture(scene: &Simulation, domain: Domain) -> Option<Self> {
        let world = scene.world.clone();

        let limits = Limits::new(scene);

        Some(match domain {
            Domain::Static => Self::Static { world, limits },
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
            Self::Static { world, limits } => {
                let mut transaction = Transaction::new(&world);

                for voxel in voxels {
                    transaction.remove(voxel)?;
                }

                let patch = StaticDraft::prepare(&world, cache, transaction, limits)
                    .await?
                    .finish()
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
                    .remove_async(cache, &world, &voxels, limits.bodies)
                    .await?;

                Ok(Patch::Dynamic {
                    id,
                    base: geometry,
                    replacements,
                })
            }
        }
    }
}

pub(in crate::simulation) enum Patch {
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
    pub(in crate::simulation) fn publish(&mut self, scene: &mut Simulation) -> Result<Vec<Body>> {
        match self {
            Self::Local(patch) => patch.publish(scene),
            Self::StructuralRequired => Err(SimulationError::Invalid),
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
                    .map_err(|_| SimulationError::StaleEdit)?;

                let current = &scene.bodies[index];

                if !Arc::ptr_eq(base, &current.geometry) {
                    return Err(SimulationError::StaleEdit);
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

fn reserve(scene: &Simulation, removed: usize, added: usize, identities: usize) -> Result<u64> {
    if added
        > scene
            .settings
            .max_bodies
            .saturating_sub(scene.bodies.len() - removed)
    {
        return Err(SimulationError::Limit("dynamic bodies"));
    }

    scene
        .next_body
        .checked_add(identities as u64)
        .ok_or(SimulationError::Limit("body identities"))
}

pub(super) fn wake_bounds(scene: &mut Simulation, min: Vec3, max: Vec3) {
    let contacts = scene
        .contacts
        .get_or_init(|| dynamics::Contacts::new(&scene.bodies));

    contacts.wake(
        &mut scene.bodies,
        min,
        max,
        scene.settings.physics.contact_slop,
    );
}

pub(super) fn dirty_keys(
    dirty: &mut BTreeSet<[i32; 3]>,
    min: VoxelCoord,
    max: VoxelCoord,
    edge: i32,
) {
    let min = IVec3::from_array((min - IVec3::ONE).to_array().map(|v| v.div_euclid(edge)));

    let max = IVec3::from_array((max + IVec3::ONE).to_array().map(|v| v.div_euclid(edge)));

    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                dirty.insert([x, y, z]);
            }
        }
    }
}
