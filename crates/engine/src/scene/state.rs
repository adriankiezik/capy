use crate::{
    scene::{
        Body, Result, SceneError, SceneSettings, dynamics, geometry, geometry::MeshSource, support,
    },
    world::{Transaction, VoxelCoord, WorldRead, prepare},
};
use glam::IVec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

#[derive(Debug)]
pub struct Scene {
    pub(crate) world: WorldRead,
    pub(crate) meshes: BTreeMap<[i32; 3], Arc<MeshSource>>,
    pub(crate) bodies: Vec<Body>,
    pub(crate) settings: SceneSettings,
    next_body: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct SceneStats {
    pub epoch: u64,
    pub storage_leaves: usize,
    pub render_leaves: usize,
    pub bodies: usize,
    pub sleeping_bodies: usize,
    pub dynamic_mass: f32,
}

impl Scene {
    pub fn new(settings: SceneSettings) -> Result<Self> {
        settings.validate()?;

        Ok(Self {
            world: WorldRead::new(settings.world.clone())?,
            meshes: BTreeMap::new(),
            bodies: Vec::new(),
            settings,
            next_body: 1,
        })
    }

    pub fn snapshot(&self) -> WorldRead {
        self.world.clone()
    }

    pub fn transaction(&self) -> Transaction {
        Transaction::new(&self.world)
    }

    pub fn stats(&self) -> SceneStats {
        SceneStats {
            epoch: self.world.epoch(),
            storage_leaves: self.world.leaf_count(),
            render_leaves: self.meshes.len()
                + self
                    .bodies
                    .iter()
                    .map(|b| b.geometry.meshes.len())
                    .sum::<usize>(),
            bodies: self.bodies.len(),
            sleeping_bodies: self.bodies.iter().filter(|b| b.sleeping).count(),
            dynamic_mass: self.bodies.iter().map(|b| b.geometry.mass).sum(),
        }
    }

    pub fn commit(&mut self, transaction: Transaction) -> Result<()> {
        let (world, mut changed) = prepare(&self.world, transaction)?;

        if changed.is_empty() {
            return Ok(());
        }

        let mut next_body = self.next_body;

        let affected = support::affected(&self.world, &world, &changed);

        let (world, additions, removed) = support::detach(
            &world,
            &affected,
            &mut next_body,
            self.settings.render_leaf_edge,
            self.settings.max_bodies.saturating_sub(self.bodies.len()),
        )?;

        changed.extend(removed);

        let dirty = dirty_keys(&changed, self.settings.render_leaf_edge);

        let mut meshes = self.meshes.clone();

        for key in dirty {
            if world.has_render_voxels(key, self.settings.render_leaf_edge) {
                meshes.insert(key, Arc::new(MeshSource::default()));
            } else {
                meshes.remove(&key);
            }
        }

        self.world = world;
        self.meshes = meshes;

        self.bodies.extend(additions);

        self.next_body = next_body;

        for body in &mut self.bodies {
            body.sleeping = false;
        }

        Ok(())
    }

    pub fn advance(&mut self, delta: Duration) -> Result<()> {
        if delta > Duration::from_secs(1) {
            return Err(SceneError::Invalid);
        }

        if !delta.is_zero() {
            self.bodies = dynamics::step(
                &self.world,
                &self.bodies,
                &self.settings.simulation,
                delta.as_secs_f32(),
            )?;
        }

        Ok(())
    }
}

fn dirty_keys(changed: &[VoxelCoord], edge: i32) -> BTreeSet<[i32; 3]> {
    let mut dirty = BTreeSet::new();

    for &p in changed {
        for z in -1..=1 {
            for y in -1..=1 {
                for x in -1..=1 {
                    dirty.insert(geometry::key(p + IVec3::new(x, y, z), edge));
                }
            }
        }
    }

    dirty
}
