use crate::world::{Material, Owner, Voxel, VoxelBounds};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chunk {
    pub coordinate: [i32; 3],
    pub palette: Vec<Voxel>,
    pub indices: Vec<u16>,
}

impl Chunk {
    pub fn voxel(&self, index: usize) -> Option<Voxel> {
        if index >= 512 {
            return None;
        }

        let palette = if self.indices.is_empty() {
            0
        } else {
            *self.indices.get(index)? as usize
        };

        self.palette.get(palette).copied()
    }

    pub fn valid(&self) -> bool {
        !self.palette.is_empty()
            && self.palette.len() <= 512
            && (self.indices.is_empty() && self.palette.len() == 1
                || self.indices.len() == 512
                    && self
                        .indices
                        .iter()
                        .all(|&i| (i as usize) < self.palette.len()))
            && self
                .coordinate
                .iter()
                .all(|v| (-125_001..=125_001).contains(v))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BodyState {
    pub id: u64,
    pub translation: [f32; 3],
    pub velocity: f32,
    pub sleeping: bool,
    pub geometry_revision: u64,
    pub geometry: Option<Vec<Chunk>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub bounds: VoxelBounds,
    pub materials: Vec<Material>,
    pub owners: Vec<Owner>,
    pub chunks: Vec<Chunk>,
    pub bodies: Vec<BodyState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldDelta {
    pub chunks: Vec<Chunk>,
    pub removed_chunks: Vec<[i32; 3]>,
    pub bodies: Vec<BodyState>,
    pub removed_bodies: Vec<u64>,
}
