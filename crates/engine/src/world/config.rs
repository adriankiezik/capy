use crate::world::{Material, Owner, VoxelBounds};

#[derive(Clone, Debug)]
pub struct WorldSettings {
    pub bounds: VoxelBounds,
    pub materials: Vec<Material>,
    pub owners: Vec<Owner>,
    pub max_leaves: usize,
    pub max_edit_voxels: usize,
    pub max_support_voxels: usize,
}
