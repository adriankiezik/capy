pub(super) mod patch;
mod worker;

use crate::{
    scene::{Body, EditSettings, Result, Scene, SceneError, Target},
    world::{VOXEL_SIZE, VoxelCoord, WorldSettings},
};
use patch::{Input, Patch};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, mpsc},
};
use worker::Workers;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Domain {
    Static,
    Body(u64),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Command {
    domain: Domain,
    voxel: [i32; 3],
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
    commands: Vec<Command>,
    receiver: mpsc::Receiver<Result<Patch>>,
}

pub struct SceneEdits {
    settings: EditSettings,
    world_settings: Arc<WorldSettings>,
    workers: Workers,
    pending: VecDeque<Command>,
    accepted: BTreeSet<Command>,
    flights: Vec<Flight>,
}

impl SceneEdits {
    pub fn new(scene: &Scene, settings: EditSettings) -> Result<Self> {
        settings.validate()?;

        Ok(Self {
            settings,
            world_settings: scene.world.root.settings.clone(),
            workers: Workers::new(settings.workers)?,
            pending: VecDeque::new(),
            accepted: BTreeSet::new(),
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

    pub fn update(&mut self, scene: &mut Scene) -> Result<Vec<EditOutcome>> {
        if !Arc::ptr_eq(&self.world_settings, &scene.world.root.settings) {
            return Err(SceneError::Invalid);
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
                Err(mpsc::TryRecvError::Disconnected) => Err(SceneError::EditWorkerStopped),
            };

            let flight = self.flights.remove(index);

            for command in &flight.commands {
                self.accepted.remove(command);
            }

            let result = result.and_then(|mut patch| {
                let result = patch.publish(scene);

                if let Ok(replacements) = &result {
                    self.retarget(scene, flight.domain, replacements);
                }

                self.workers.retire(patch);

                result.map(|_| ())
            });

            outcomes.push(EditOutcome {
                targets: flight.commands.into_iter().map(Target::from).collect(),
                result,
            });
        }

        while self.flights.len() < self.settings.workers {
            let Some(domain) = self
                .pending
                .iter()
                .map(|command| command.domain)
                .find(|domain| !self.flights.iter().any(|flight| flight.domain == *domain))
            else {
                break;
            };

            let Some(input) = Input::capture(scene, domain) else {
                let mut targets = Vec::new();

                self.pending.retain(|command| {
                    if command.domain == domain {
                        self.accepted.remove(command);

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
                .filter(|command| command.domain == domain)
                .take(limit)
                .copied()
                .collect();

            let voxels = commands
                .iter()
                .map(|command| VoxelCoord::from_array(command.voxel))
                .collect();

            let Some(receiver) = self.workers.submit(input, voxels)? else {
                break;
            };

            let selected: BTreeSet<_> = commands.iter().copied().collect();

            self.pending.retain(|command| !selected.contains(command));

            self.flights.push(Flight {
                domain,
                commands,
                receiver,
            });
        }

        Ok(outcomes)
    }

    fn retarget(&mut self, scene: &Scene, domain: Domain, replacements: &[Body]) {
        let mut remapped = Vec::new();

        self.pending.retain(|command| {
            if command.domain != domain {
                return true;
            }

            self.accepted.remove(command);

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
