use super::VoxelCoord;
use super::{MaterialId, Voxel, VoxelBounds};
use capy_engine_protocol::message::Chunk;
#[cfg(test)]
use glam::IVec3;
use std::{collections::BTreeMap, sync::Arc};

pub(crate) const LEAF_EDGE: i32 = 8;

#[derive(Clone, Debug)]
pub(crate) struct Leaf(pub(crate) Chunk);

impl Leaf {
    pub(crate) fn voxel(&self, index: usize) -> Voxel {
        self.0.voxel(index).unwrap_or(Voxel::EMPTY)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WorldRead {
    pub(crate) leaves: im::OrdMap<[i32; 3], Arc<Leaf>>,
    pub(crate) materials: Arc<BTreeMap<MaterialId, [f32; 3]>>,
    pub(crate) bounds: VoxelBounds,
}

impl WorldRead {
    pub(crate) fn material(&self, id: MaterialId) -> Option<&[f32; 3]> {
        self.materials.get(&id)
    }
}

pub(crate) fn address(voxel: VoxelCoord) -> ([i32; 3], usize) {
    let key = voxel.to_array().map(|v| v.div_euclid(LEAF_EDGE));

    let [x, y, z] = voxel.to_array().map(|v| v.rem_euclid(LEAF_EDGE) as usize);

    (key, x + 8 * (y + 8 * z))
}

#[cfg(test)]
pub(crate) fn coordinate(key: [i32; 3], index: usize) -> VoxelCoord {
    IVec3::from_array(key) * LEAF_EDGE
        + IVec3::new(
            (index % 8) as i32,
            (index / 8 % 8) as i32,
            (index / 64) as i32,
        )
}
