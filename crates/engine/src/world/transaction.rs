use crate::world::storage::{LEAF_VOXELS, address};
use crate::world::{Result, Voxel, VoxelCoord, WorldError, WorldRead};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug)]
pub struct Transaction {
    base: WorldRead,
    edits: BTreeMap<[i32; 3], (Voxel, Voxel)>,
    failure: Option<WorldError>,
}

impl Transaction {
    pub fn new(snapshot: &WorldRead) -> Self {
        Self {
            base: snapshot.clone(),
            edits: BTreeMap::new(),
            failure: None,
        }
    }

    pub fn original_voxel(&self, coordinate: VoxelCoord) -> Result<Voxel> {
        self.base.voxel(coordinate)
    }

    pub fn place(&mut self, coordinate: VoxelCoord, value: Voxel) -> Result<()> {
        self.write(coordinate, Voxel::EMPTY, value)
    }

    pub fn remove(&mut self, coordinate: VoxelCoord) -> Result<()> {
        match self.base.voxel(coordinate) {
            Ok(voxel) => self.write(coordinate, voxel, Voxel::EMPTY),
            Err(error) => {
                self.failure = Some(error.clone());

                Err(error)
            }
        }
    }

    pub fn replace(&mut self, coordinate: VoxelCoord, expected: Voxel, value: Voxel) -> Result<()> {
        self.write(coordinate, expected, value)
    }

    fn write(&mut self, coordinate: VoxelCoord, expected: Voxel, value: Voxel) -> Result<()> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }

        let result = self.write_inner(coordinate, expected, value);

        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }

        result
    }

    fn write_inner(&mut self, coordinate: VoxelCoord, expected: Voxel, value: Voxel) -> Result<()> {
        if self.edits.len() >= self.base.root.settings.max_edit_voxels {
            return Err(WorldError::Limit("transaction voxels"));
        }

        if self.base.voxel(coordinate)? != expected {
            return Err(WorldError::Conflict(coordinate));
        }

        if !value.is_empty()
            && (self.base.owner(value.owner).is_none()
                || self.base.material(value.material).is_none())
        {
            return Err(WorldError::Invalid);
        }

        if self.edits.contains_key(&coordinate.to_array()) {
            return Err(WorldError::Duplicate(coordinate));
        }

        self.edits.insert(coordinate.to_array(), (expected, value));

        Ok(())
    }
}

pub(crate) fn prepare(
    current: &WorldRead,
    transaction: Transaction,
) -> Result<(WorldRead, Vec<VoxelCoord>)> {
    if !Arc::ptr_eq(&current.root, &transaction.base.root) {
        return Err(WorldError::Stale);
    }

    if let Some(error) = transaction.failure {
        return Err(error);
    }

    let mut leaves = BTreeMap::new();

    let mut changed = Vec::new();

    for (coordinate, (expected, value)) in transaction.edits {
        let coordinate = VoxelCoord::from_array(coordinate);

        if current.voxel(coordinate)? != expected {
            return Err(WorldError::Conflict(coordinate));
        }

        if expected == value {
            continue;
        }

        let (key, index) = address(coordinate);

        let voxels = leaves.entry(key).or_insert_with(|| {
            current
                .leaf(key)
                .map_or_else(|| vec![Voxel::EMPTY; LEAF_VOXELS], |leaf| leaf.decode())
        });

        voxels[index] = value;

        changed.push(coordinate);
    }

    if changed.is_empty() {
        return Ok((current.clone(), changed));
    }

    Ok((current.replace_leaves(leaves)?, changed))
}
