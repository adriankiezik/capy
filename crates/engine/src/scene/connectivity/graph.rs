use super::{
    cache::Connectivity,
    partition::{EMPTY, Mask, Partition},
};
use crate::{
    scene::{Result, SceneError},
    world::{Leaf, LeafCoord, VoxelCoord, WorldRead, address},
};
use glam::IVec3;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::Arc,
};

pub(super) const DIRECTIONS: [IVec3; 6] = [
    IVec3::X,
    IVec3::NEG_X,
    IVec3::Y,
    IVec3::NEG_Y,
    IVec3::Z,
    IVec3::NEG_Z,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Node {
    pub(super) leaf: LeafCoord,
    pub(super) component: u16,
}

#[derive(Default)]
pub(in crate::scene) struct Group {
    pub(super) leaves: BTreeMap<LeafCoord, Mask>,
}

#[derive(Clone, Copy)]
pub(super) enum Relation {
    Bond,
    Support,
    Dependents,
}

pub(super) struct Graph<'a, F> {
    cache: &'a mut Connectivity,
    world: &'a WorldRead,
    source: F,
    charged: HashSet<Node>,
    work: usize,
}

impl<'a, 'b, F: Fn(LeafCoord) -> Option<&'b Arc<Leaf>>> Graph<'a, F> {
    pub(super) fn new(cache: &'a mut Connectivity, world: &'a WorldRead, source: F) -> Self {
        Self {
            cache,
            world,
            source,
            charged: HashSet::new(),
            work: 0,
        }
    }

    pub(super) fn partition(&mut self, key: LeafCoord) -> Result<Option<Arc<Partition>>> {
        (self.source)(key)
            .map(|leaf| self.cache.partition(self.world, leaf))
            .transpose()
    }

    pub(super) fn node(&mut self, p: VoxelCoord) -> Result<Option<Node>> {
        let (leaf, index) = address(p);

        Ok(self.partition(leaf)?.and_then(|partition| {
            let component = partition.labels[index];

            (component != EMPTY).then_some(Node { leaf, component })
        }))
    }

    pub(super) fn charge(&mut self, node: Node) -> Result<()> {
        if self.charged.insert(node) {
            let partition = self.partition(node.leaf)?.ok_or(SceneError::Invalid)?;

            self.work += partition.components[node.component as usize].cells.count();

            if self.work > self.world.root.settings.max_support_voxels {
                return Err(SceneError::Limit("support scan"));
            }
        }

        Ok(())
    }

    pub(super) fn neighbors(&mut self, node: Node, relation: Relation) -> Result<Vec<Node>> {
        let partition = self.partition(node.leaf)?.ok_or(SceneError::Invalid)?;

        let component = &partition.components[node.component as usize];

        let mut output = Vec::new();

        let internal = match relation {
            Relation::Bond => &[][..],
            Relation::Support => &component.down,
            Relation::Dependents => &component.up,
        };

        output.extend(internal.iter().map(|&component| Node {
            leaf: node.leaf,
            component,
        }));

        for (face, direction) in DIRECTIONS.iter().enumerate() {
            let mut bits = component.faces[face];

            if bits == 0 {
                continue;
            }

            let leaf = (IVec3::from_array(node.leaf) + direction).to_array();

            let Some(neighbor) = self.partition(leaf)? else {
                continue;
            };

            let contact = matches!(relation, Relation::Support) && face == 3
                || matches!(relation, Relation::Dependents) && face == 2;

            if neighbor.components.len() == 1 {
                let other = &neighbor.components[0];

                if bits & other.faces[face ^ 1] != 0
                    && (other.structure == component.structure || contact)
                {
                    output.push(Node { leaf, component: 0 });
                }

                continue;
            }

            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;

                bits &= bits - 1;

                let axis = face / 2;

                let mut p = [0; 3];

                p[axis] = if face % 2 == 0 { 0 } else { 7 };
                p[(axis + 1) % 3] = bit % 8;
                p[(axis + 2) % 3] = bit / 8;

                let label = neighbor.labels[p[0] + p[1] * 8 + p[2] * 64];

                if label == EMPTY {
                    continue;
                }

                if neighbor.components[label as usize].structure == component.structure || contact {
                    output.push(Node {
                        leaf,
                        component: label,
                    });
                }
            }
        }

        output.sort_unstable();

        output.dedup();

        Ok(output)
    }

    pub(super) fn group(&mut self, nodes: impl IntoIterator<Item = Node>) -> Result<Group> {
        let mut group = Group::default();

        for node in nodes {
            let partition = self.partition(node.leaf)?.ok_or(SceneError::Invalid)?;

            group
                .leaves
                .entry(node.leaf)
                .or_default()
                .extend(partition.components[node.component as usize].cells);
        }

        Ok(group)
    }

    pub(super) fn groups(&mut self, mut remaining: BTreeSet<Node>) -> Result<Vec<Group>> {
        let mut groups = Vec::new();

        while let Some(seed) = remaining.pop_first() {
            let mut pending = vec![seed];

            let mut nodes = Vec::new();

            while let Some(node) = pending.pop() {
                nodes.push(node);

                for neighbor in self.neighbors(node, Relation::Bond)? {
                    if remaining.remove(&neighbor) {
                        pending.push(neighbor);
                    }
                }
            }

            groups.push(self.group(nodes)?);
        }

        Ok(groups)
    }
}
