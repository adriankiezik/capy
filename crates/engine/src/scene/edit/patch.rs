use super::Domain;
use crate::{
    scene::{
        Body, Result, Scene, SceneError,
        body::BodyGeometry,
        connectivity::Connectivity,
        geometry::{self, MeshSource, MeshSources},
        support,
    },
    world::{LEAF_VOXELS, Transaction, VoxelCoord, WorldRead, coordinate, prepare},
};
use glam::IVec3;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy)]
pub(in crate::scene) struct Limits {
    pub(super) edge: i32,
    pub(in crate::scene) bodies: usize,
    pub(super) vertices: usize,
}

impl Limits {
    pub(in crate::scene) fn new(scene: &Scene) -> Self {
        Self {
            edge: scene.settings.render_leaf_edge,
            bodies: scene.settings.max_bodies,
            vertices: scene.settings.max_mesh_vertices,
        }
    }
}

pub(in crate::scene) struct StaticPatch {
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
            support::detach(cache, &world, &changed, limits.edge, limits.bodies)?;

        for body in &mut additions {
            Arc::get_mut(&mut body.geometry)
                .ok_or(SceneError::Invalid)?
                .inherit_meshes(base, sources, body.translation, limits.edge);
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

    fn warm(&self, limits: Limits) -> Result<()> {
        let mut available = limits.vertices;

        for key in &self.dirty {
            if let Some(source) = self.meshes.get(key) {
                let mesh = source.resolve(
                    *key,
                    limits.edge,
                    |key| self.world.leaf(key).map(Arc::as_ref),
                    &self.world,
                    available,
                )?;

                available -= mesh.vertices.len();
            }
        }

        for body in &self.additions {
            body.geometry
                .prepare_meshes(&self.world, limits.edge, &mut available)?;
        }

        Ok(())
    }

    pub(in crate::scene) fn publish(&mut self, scene: &mut Scene) -> Result<Vec<Body>> {
        if !Arc::ptr_eq(&self.base.root, &scene.world.root) {
            return Err(SceneError::StaleEdit);
        }

        if Arc::ptr_eq(&self.base.root, &self.world.root) {
            return Ok(Vec::new());
        }

        let next = reserve(scene, 0, self.additions.len(), self.additions.len())?;

        for (offset, body) in self.additions.iter_mut().enumerate() {
            body.id = scene.next_body + offset as u64;
        }

        std::mem::swap(&mut scene.world, &mut self.world);

        std::mem::swap(&mut scene.meshes, &mut self.meshes);

        scene.bodies.extend(self.additions.iter().cloned());

        scene.next_body = next;

        wake(scene);

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
                geometry: scene
                    .bodies
                    .iter()
                    .find(|body| body.id == id)?
                    .geometry
                    .clone(),
                limits,
            },
        })
    }

    pub(super) fn prepare(
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

                let patch = StaticPatch::prepare(&world, &meshes, cache, transaction, limits)?;

                patch.warm(limits)?;

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

                let replacements =
                    geometry.remove(cache, &world, &voxels, limits.edge, limits.bodies)?;

                let mut available = limits.vertices;

                for geometry in &replacements {
                    geometry.prepare_meshes(&world, limits.edge, &mut available)?;
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
            Self::Static(patch) => patch.publish(scene),
            Self::Dynamic {
                id,
                base,
                replacements,
            } => {
                let index = scene
                    .bodies
                    .iter()
                    .position(|body| body.id == *id)
                    .ok_or(SceneError::StaleEdit)?;

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

                wake(scene);

                Ok(bodies)
            }
        }
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

fn wake(scene: &mut Scene) {
    for body in &mut scene.bodies {
        body.sleeping = false;
    }
}

fn dirty_keys(dirty: &mut BTreeSet<[i32; 3]>, min: VoxelCoord, max: VoxelCoord, edge: i32) {
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
