use crate::{
    simulation::{
        Body, Result, SimulationConfig, SimulationError, Target,
        connectivity::Connectivity,
        dynamics,
        edit::{
            Limits,
            patch::{Patch, StaticPatch},
        },
    },
    world::{Transaction, WorldRead},
};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Simulation {
    pub(crate) world: WorldRead,
    pub(crate) bodies: Vec<Body>,
    pub(crate) settings: SimulationConfig,
    pub(super) next_body: u64,
    pub(super) contacts: Arc<OnceLock<dynamics::Contacts>>,
    connectivity: Connectivity,
}

#[derive(Clone, Copy, Debug)]
pub struct SimulationStats {
    pub epoch: u64,
    pub storage_leaves: usize,
    pub bodies: usize,
    pub sleeping_bodies: usize,
    pub dynamic_mass: f32,
}

impl Simulation {
    pub fn new(settings: SimulationConfig) -> Result<Self> {
        settings.validate()?;

        Ok(Self {
            world: WorldRead::new(settings.world.clone())?,
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

    pub fn stats(&self) -> SimulationStats {
        SimulationStats {
            epoch: self.world.epoch(),
            storage_leaves: self.world.leaf_count(),
            bodies: self.bodies.len(),
            sleeping_bodies: self.bodies.iter().filter(|body| body.sleeping).count(),
            dynamic_mass: self.bodies.iter().map(|body| body.geometry.mass).sum(),
        }
    }

    pub fn commit(&mut self, transaction: Transaction) -> Result<()> {
        let mut limits = Limits::new(self);

        limits.bodies = self.settings.max_bodies.saturating_sub(self.bodies.len());

        let mut patch =
            StaticPatch::prepare(&self.world, &mut self.connectivity, transaction, limits)?;

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
            .ok_or(SimulationError::Invalid)?;

        let replacements = body.geometry.remove(
            &mut self.connectivity,
            &self.world,
            &[voxel],
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
            return Err(SimulationError::Invalid);
        }

        if !delta.is_zero() {
            let moving = self.bodies.iter().any(|body| !body.sleeping);

            self.bodies = dynamics::step(
                &self.world,
                &self.bodies,
                &self.settings.physics,
                delta.as_secs_f32(),
            )?;

            if moving {
                self.contacts = Arc::default();
            }
        }

        Ok(())
    }
}
