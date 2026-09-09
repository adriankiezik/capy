#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use super::{
    patch::{Input, Patch},
    worker::{Receipt, Workers},
};
use crate::{
    simulation::{Body, EditSettings, Result, Simulation, SimulationError, Target},
    world::{VOXEL_SIZE, VoxelCoord, WorldSettings},
};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, mpsc},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::simulation) enum Domain {
    Static,
    Body(u64),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Command {
    domain: Domain,
    voxel: [i32; 3],
}

impl Command {
    fn chunk(self, edge: i32) -> [i32; 3] {
        self.voxel.map(|v| v.div_euclid(edge))
    }

    fn structural_key(self, edge: i32) -> (Domain, [i32; 3]) {
        (
            self.domain,
            if self.domain == Domain::Static {
                self.chunk(edge)
            } else {
                [0; 3]
            },
        )
    }
}

impl From<Target> for Command {
    fn from(target: Target) -> Self {
        match target {
            Target::Static(voxel) => Self {
                domain: Domain::Static,
                voxel: voxel.to_array(),
            },
            Target::Dynamic { body, voxel } => Self {
                domain: Domain::Body(body),
                voxel: voxel.to_array(),
            },
        }
    }
}

impl From<Command> for Target {
    fn from(command: Command) -> Self {
        let voxel = VoxelCoord::from_array(command.voxel);

        match command.domain {
            Domain::Static => Self::Static(voxel),
            Domain::Body(body) => Self::Dynamic { body, voxel },
        }
    }
}

#[derive(Debug)]
pub struct EditOutcome {
    pub targets: Vec<Target>,
    pub result: Result<()>,
}

struct Flight {
    domain: Domain,
    structural: bool,
    chunk: [i32; 3],
    commands: Vec<Command>,
    receiver: Receipt,
}

pub struct SimulationEdits {
    settings: EditSettings,
    world_settings: Arc<WorldSettings>,
    workers: Workers,
    pending: VecDeque<Command>,
    accepted: BTreeSet<Command>,
    structural: BTreeSet<(Domain, [i32; 3])>,
    flights: Vec<Flight>,
}

impl Drop for SimulationEdits {
    fn drop(&mut self) {
        self.flights.clear();
    }
}

impl SimulationEdits {
    pub fn new(scene: &Simulation, settings: EditSettings) -> Result<Self> {
        settings.validate()?;

        Ok(Self {
            settings,
            world_settings: scene.world.root.settings.clone(),
            workers: Workers::new(settings.workers)?,
            pending: VecDeque::new(),
            accepted: BTreeSet::new(),
            structural: BTreeSet::new(),
            flights: Vec::new(),
        })
    }

    pub fn queue_remove(&mut self, target: Target) -> bool {
        let command = Command::from(target);

        if self.accepted.contains(&command) {
            return true;
        }

        if self.accepted.len() >= self.settings.max_pending {
            return false;
        }

        self.accepted.insert(command);

        self.pending.push_back(command);

        true
    }

    pub fn pending(&self) -> usize {
        self.accepted.len()
    }

    pub fn update(&mut self, scene: &mut Simulation) -> Result<Vec<EditOutcome>> {
        if !Arc::ptr_eq(&self.world_settings, &scene.world.root.settings) {
            return Err(SimulationError::Invalid);
        }

        let mut outcomes = Vec::new();

        let mut index = 0;

        while index < self.flights.len() {
            let result = match self.flights[index].receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;

                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => Err(SimulationError::EditWorkerStopped),
            };

            let flight = self.flights.remove(index);

            let mut structural = flight.structural;

            let result = result.and_then(|mut patch| {
                if matches!(patch, Patch::StructuralRequired) {
                    structural = true;

                    return Err(SimulationError::StaleEdit);
                }

                let result = patch.publish(scene);

                if flight.structural
                    && let Ok(replacements) = &result
                {
                    self.retarget(scene, flight.domain, replacements);
                }

                self.workers.retire(patch);

                result.map(|_| ())
            });

            if matches!(result, Err(SimulationError::StaleEdit)) {
                for command in flight.commands {
                    if structural {
                        self.structural
                            .insert(command.structural_key(crate::world::LEAF_EDGE));
                    }

                    self.pending.push_back(command);
                }

                continue;
            }

            for command in &flight.commands {
                self.accepted.remove(command);

                self.structural
                    .remove(&command.structural_key(crate::world::LEAF_EDGE));
            }

            outcomes.push(EditOutcome {
                targets: flight.commands.into_iter().map(Target::from).collect(),
                result,
            });
        }

        while self.flights.len() < self.settings.workers * 2 {
            let Some(first) = self
                .pending
                .iter()
                .find(|command| {
                    let edge = crate::world::LEAF_EDGE;

                    let structural = self.requires_structural(**command, edge);

                    !self.flights.iter().any(|flight| {
                        flight.domain == command.domain
                            && ((0..3).all(|axis| {
                                (flight.chunk[axis] - command.chunk(edge)[axis]).abs() <= 1
                            }) || command.domain != Domain::Static
                                && (structural || flight.structural))
                    })
                })
                .copied()
            else {
                break;
            };

            let domain = first.domain;

            let structural = self.requires_structural(first, crate::world::LEAF_EDGE)
                || matches!(domain, Domain::Body(id) if scene.bodies.binary_search_by_key(&id, |body| body.id)
                    .is_ok_and(|index| scene.bodies[index].geometry.leaves.len() == 1));

            let chunk = first.chunk(crate::world::LEAF_EDGE);

            let Some(input) = Input::capture(scene, domain) else {
                let mut targets = Vec::new();

                self.pending.retain(|command| {
                    if command.domain == domain {
                        self.accepted.remove(command);

                        self.structural
                            .remove(&command.structural_key(crate::world::LEAF_EDGE));

                        targets.push(Target::from(*command));

                        false
                    } else {
                        true
                    }
                });

                outcomes.push(EditOutcome {
                    targets,
                    result: Ok(()),
                });

                continue;
            };

            let limit = self
                .settings
                .max_batch
                .min(self.world_settings.max_edit_voxels);

            let commands: Vec<_> = self
                .pending
                .iter()
                .filter(|command| {
                    command.domain == domain
                        && (structural && domain != Domain::Static
                            || command.chunk(crate::world::LEAF_EDGE) == chunk)
                })
                .take(limit)
                .copied()
                .collect();

            let voxels = commands
                .iter()
                .map(|command| VoxelCoord::from_array(command.voxel))
                .collect();

            let Some(receiver) = self.workers.submit(input, voxels, structural)? else {
                break;
            };

            let selected: BTreeSet<_> = commands.iter().copied().collect();

            self.pending.retain(|command| !selected.contains(command));

            self.flights.push(Flight {
                domain,
                structural,
                chunk,
                commands,
                receiver,
            });
        }

        Ok(outcomes)
    }

    fn requires_structural(&self, command: Command, edge: i32) -> bool {
        self.structural.contains(&command.structural_key(edge))
    }

    fn retarget(&mut self, scene: &Simulation, domain: Domain, replacements: &[Body]) {
        let mut index = 0;

        while index < self.flights.len() {
            let flight = &self.flights[index];

            let transferred = domain == Domain::Static
                && flight.domain == domain
                && flight.commands.iter().any(|command| {
                    let voxel = VoxelCoord::from_array(command.voxel);

                    replacements.iter().any(|body| {
                        !body
                            .geometry
                            .voxel(voxel - (body.translation / VOXEL_SIZE).round().as_ivec3())
                            .is_empty()
                    })
                });

            if transferred {
                let flight = self.flights.remove(index);

                drop(flight.receiver);

                self.pending.extend(flight.commands);
            } else {
                index += 1;
            }
        }

        let mut remapped = Vec::new();

        self.pending.retain(|command| {
            if command.domain != domain {
                return true;
            }

            self.accepted.remove(command);

            self.structural
                .remove(&command.structural_key(crate::world::LEAF_EDGE));

            let voxel = VoxelCoord::from_array(command.voxel);

            if domain == Domain::Static && !scene.world.resident_voxel(voxel).is_empty() {
                remapped.push(*command);
            } else if let Some(replacement) = replacements.iter().find_map(|body| {
                let local = if domain == Domain::Static {
                    voxel - (body.translation / VOXEL_SIZE).round().as_ivec3()
                } else {
                    voxel
                };

                (!body.geometry.voxel(local).is_empty()).then_some(Command {
                    domain: Domain::Body(body.id),
                    voxel: local.to_array(),
                })
            }) {
                remapped.push(replacement);
            }

            false
        });

        for command in remapped {
            if self.accepted.insert(command) {
                self.pending.push_back(command);
            }
        }
    }
}
