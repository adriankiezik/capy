use crate::{
    scene::{
        Body, Result, SceneError, SceneSettings, Target,
        connectivity::Connectivity,
        dynamics,
        edit::patch::{Limits, Patch, StaticPatch},
        geometry::MeshSources,
    },
    world::{Transaction, WorldRead},
};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Scene {
    pub(crate) world: WorldRead,
    pub(crate) meshes: Arc<MeshSources>,
    pub(crate) bodies: Vec<Body>,
    pub(crate) settings: SceneSettings,
    pub(super) next_body: u64,
    pub(super) contacts: Arc<OnceLock<dynamics::Contacts>>,
    connectivity: Connectivity,
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
            meshes: Arc::default(),
            bodies: Vec::new(),
            settings,
            next_body: 1,
            contacts: Arc::default(),
            connectivity: Connectivity::default(),
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
                    .map(|body| body.geometry.meshes.len())
                    .sum::<usize>(),
            bodies: self.bodies.len(),
            sleeping_bodies: self.bodies.iter().filter(|body| body.sleeping).count(),
            dynamic_mass: self.bodies.iter().map(|body| body.geometry.mass).sum(),
        }
    }

    pub fn commit(&mut self, transaction: Transaction) -> Result<()> {
        let mut limits = Limits::new(self);

        limits.bodies = self.settings.max_bodies.saturating_sub(self.bodies.len());

        let mut patch = StaticPatch::prepare(
            &self.world,
            &self.meshes,
            &mut self.connectivity,
            transaction,
            limits,
        )?;

        patch.publish(self)?;

        Ok(())
    }

    pub fn remove(&mut self, target: Target) -> Result<()> {
        let (id, voxel) = match target {
            Target::Static(voxel) => {
                let mut transaction = self.transaction();

                transaction.remove(voxel)?;

                return self.commit(transaction);
            }
            Target::Dynamic { body, voxel } => (body, voxel),
        };

        let body = self
            .bodies
            .iter()
            .find(|body| body.id == id)
            .ok_or(SceneError::Invalid)?;

        let replacements = body.geometry.remove(
            &mut self.connectivity,
            &self.world,
            &[voxel],
            self.settings.render_leaf_edge,
            self.settings.max_bodies - (self.bodies.len() - 1),
        )?;

        Patch::Dynamic {
            id,
            base: body.geometry.clone(),
            replacements,
        }
        .publish(self)?;

        Ok(())
    }

    pub fn advance(&mut self, delta: Duration) -> Result<()> {
        if delta > Duration::from_secs(1) {
            return Err(SceneError::Invalid);
        }

        if !delta.is_zero() {
            let moving = self.bodies.iter().any(|body| !body.sleeping);

            self.bodies = dynamics::step(
                &self.world,
                &self.bodies,
                &self.settings.simulation,
                delta.as_secs_f32(),
            )?;

            if moving {
                self.contacts = Arc::default();
            }
        }

        Ok(())
    }
}
