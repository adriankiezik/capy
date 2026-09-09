use super::{
    graph::{DIRECTIONS, Graph, Group, Node, Relation},
    partition::{EMPTY, Metrics, Partition},
};
use crate::{
    simulation::{Result, SimulationError, work::Work},
    world::{
        LEAF_EDGE, LEAF_VOXELS, Leaf, LeafCoord, LeafSources, Voxel, VoxelCoord, WorldRead, address,
    },
};
use glam::IVec3;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::{Arc, Weak},
};

#[derive(Clone, Debug)]
struct CachedPartition {
    source: Weak<Leaf>,
    partition: Arc<Partition>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct Connectivity {
    partitions: im::HashMap<usize, CachedPartition>,
    edits: usize,
}

impl Connectivity {
    pub(super) fn partition(
        &mut self,
        world: &WorldRead,
        leaf: &Arc<Leaf>,
    ) -> Result<Arc<Partition>> {
        let key = Arc::as_ptr(leaf) as usize;

        if let Some(cached) = self.partitions.get(&key)
            && cached.source.strong_count() != 0
        {
            return Ok(cached.partition.clone());
        }

        let partition = Arc::new(Partition::new(world, leaf)?);

        self.partitions.insert(
            key,
            CachedPartition {
                source: Arc::downgrade(leaf),
                partition: partition.clone(),
            },
        );

        Ok(partition)
    }

    pub(in crate::simulation) fn metrics(
        &mut self,
        world: &WorldRead,
        leaf: &Arc<Leaf>,
    ) -> Result<Arc<Metrics>> {
        Ok(self.partition(world, leaf)?.metrics.clone())
    }

    async fn maintain(&mut self) {
        self.edits += 1;

        if self.edits == 64 {
            let mut work = Work::default();

            for (&key, cached) in &self.partitions.clone() {
                work.checkpoint().await;

                if cached.source.strong_count() == 0 {
                    self.partitions.remove(&key);
                }
            }

            self.edits = 0;
        }
    }

    pub(in crate::simulation) async fn unsupported(
        &mut self,
        world: &WorldRead,
        changed: &[VoxelCoord],
    ) -> Result<Vec<Group>> {
        self.maintain().await;

        let mut graph = Graph::new(self, world, |key| world.leaf(key));

        let mut leaves = BTreeSet::new();

        for &p in changed {
            let (key, _) = address(p);

            leaves.insert(key);
        }

        let changed_leaves: Vec<_> = leaves.iter().copied().collect();

        for key in changed_leaves {
            leaves.extend(DIRECTIONS.map(|d| (IVec3::from_array(key) + d).to_array()));
        }

        let mut seeds = Vec::new();

        for leaf in leaves {
            if let Some(partition) = graph.partition(leaf)? {
                seeds.extend((0..partition.components.len()).map(|component| Node {
                    leaf,
                    component: component as u16,
                }));
            }
        }

        seeds.sort_by_key(|node| std::cmp::Reverse(node.leaf[1]));

        let mut supported = HashSet::new();

        let mut unsupported = BTreeSet::new();

        while let Some(seed) = seeds.pop() {
            if supported.contains(&seed) || unsupported.contains(&seed) {
                continue;
            }

            let mut parents = HashMap::from([(seed, None)]);

            let mut pending = vec![seed];

            let mut found = None;

            while let Some(node) = pending.pop() {
                if unsupported.contains(&node) {
                    continue;
                }

                graph.charge(node).await?;

                let partition = graph
                    .partition(node.leaf)?
                    .ok_or(SimulationError::Invalid)?;

                if supported.contains(&node)
                    || node.leaf[1] * LEAF_EDGE
                        + partition.components[node.component as usize].min_y
                        == world.bounds().min.y
                {
                    found = Some(node);

                    break;
                }

                let mut neighbors = graph.neighbors(node, Relation::Support)?;

                neighbors.sort_by_key(|neighbor| std::cmp::Reverse(neighbor.leaf[1]));

                for neighbor in neighbors {
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        parents.entry(neighbor)
                    {
                        entry.insert(Some(node));

                        pending.push(neighbor);
                    }
                }
            }

            if let Some(mut node) = found {
                loop {
                    supported.insert(node);

                    let Some(parent) = parents[&node] else { break };

                    node = parent;
                }
            } else {
                for node in parents.keys().copied() {
                    if unsupported.insert(node) {
                        seeds.extend(graph.neighbors(node, Relation::Dependents)?);
                    }
                }
            }
        }

        graph.groups(unsupported).await
    }

    pub(in crate::simulation) async fn split<'a>(
        &mut self,
        world: &WorldRead,
        removed: &[VoxelCoord],
        source: impl Fn(LeafCoord) -> Option<&'a Arc<Leaf>>,
    ) -> Result<Option<Vec<Group>>> {
        self.maintain().await;

        let mut graph = Graph::new(self, world, source);

        let mut seeds = BTreeSet::new();

        for &voxel in removed {
            for direction in DIRECTIONS {
                if let Some(node) = graph.node(voxel + direction)? {
                    seeds.insert(node);
                }
            }
        }

        let Some(&start) = seeds.first() else {
            return Ok(Some(Vec::new()));
        };

        if seeds.len() == 1 {
            return Ok(None);
        }

        let mut remaining = seeds.clone();

        let mut groups = Vec::new();

        while let Some(seed) = remaining.pop_first() {
            let mut visited = BTreeSet::from([seed]);

            let mut pending = std::collections::VecDeque::from([seed]);

            while let Some(node) = pending.pop_front() {
                graph.charge(node).await?;

                remaining.remove(&node);

                if seed == start && remaining.is_empty() {
                    return Ok(None);
                }

                for neighbor in graph.neighbors(node, Relation::Bond)? {
                    if visited.insert(neighbor) {
                        pending.push_back(neighbor);
                    }
                }
            }

            groups.push(graph.group(visited).await?);
        }

        Ok(Some(groups))
    }

    pub(in crate::simulation) async fn preserves<'a>(
        &mut self,
        world: &WorldRead,
        changes: &BTreeMap<LeafCoord, Option<Arc<Leaf>>>,
        source: impl Fn(LeafCoord) -> Option<&'a Arc<Leaf>>,
    ) -> Result<bool> {
        self.maintain().await;

        let mut mapping = BTreeMap::new();

        let mut work = Work::default();

        for (&key, replacement) in changes {
            work.checkpoint().await;

            let (Some(before), Some(after)) = (source(key), replacement.as_ref()) else {
                return Ok(false);
            };

            let before = self.partition(world, before)?;

            let after = self.partition(world, after)?;

            if before.components.len() != after.components.len() {
                return Ok(false);
            }

            let mut labels = BTreeMap::new();

            for i in 0..LEAF_VOXELS {
                if after.labels[i] != EMPTY {
                    let old = before.labels[i];

                    let new = after.labels[i];

                    if old == EMPTY || labels.insert(new, old).is_some_and(|label| label != old) {
                        return Ok(false);
                    }
                }
            }

            if labels.values().copied().collect::<BTreeSet<_>>().len() != before.components.len() {
                return Ok(false);
            }

            for (new, old) in labels {
                let anchored = |partition: &Partition, label: u16| {
                    key[1] * LEAF_EDGE + partition.components[label as usize].min_y
                        == world.bounds().min.y
                };

                if anchored(&before, old) != anchored(&after, new) {
                    return Ok(false);
                }

                mapping.insert(
                    Node {
                        leaf: key,
                        component: new,
                    },
                    Node {
                        leaf: key,
                        component: old,
                    },
                );
            }
        }

        for (&after, &before) in &mapping {
            work.checkpoint().await;

            for relation in [Relation::Bond, Relation::Support, Relation::Dependents] {
                let old = Graph::new(self, world, &source).neighbors(before, relation)?;

                let mut new = Graph::new(self, world, |key| {
                    changes
                        .get(&key)
                        .map_or_else(|| source(key), Option::as_ref)
                })
                .neighbors(after, relation)?;

                for node in &mut new {
                    if let Some(original) = mapping.get(node) {
                        *node = *original;
                    }
                }

                new.sort_unstable();

                new.dedup();

                if old != new {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    pub(in crate::simulation) async fn extract<'a>(
        &mut self,
        world: &WorldRead,
        group: &Group,
        source: impl Fn(LeafCoord) -> Option<&'a Arc<Leaf>>,
    ) -> Result<LeafSources> {
        let mut leaves = LeafSources::new();

        let mut work = Work::default();

        for (&key, &mask) in &group.leaves {
            work.checkpoint().await;

            let leaf = source(key).ok_or(SimulationError::Invalid)?;

            let partition = self.partition(world, leaf)?;

            let leaf = if mask == partition.occupied {
                leaf.clone()
            } else {
                let voxels: Vec<_> = (0..LEAF_VOXELS)
                    .map(|i| {
                        if mask.contains(i) {
                            leaf.voxel(i)
                        } else {
                            Voxel::EMPTY
                        }
                    })
                    .collect();

                Arc::new(Leaf::encode(&voxels).ok_or(SimulationError::Invalid)?)
            };

            leaves.insert(key, leaf);
        }

        Ok(leaves)
    }
}
