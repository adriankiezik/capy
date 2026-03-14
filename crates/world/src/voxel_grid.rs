use crate::error::{Result, WorldError};

pub struct VoxelGrid {
    pub data: Vec<u8>,
    pub size_x: u32,
    pub size_y: u32,
    pub size_z: u32,
}

impl VoxelGrid {
    pub fn new(size_x: u32, size_y: u32, size_z: u32, data: Vec<u8>) -> Result<Self> {
        let expected = (size_x as usize) * (size_y as usize) * (size_z as usize);
        if data.len() != expected {
            return Err(WorldError::InvalidGridDimensions {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            data,
            size_x,
            size_y,
            size_z,
        })
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0
            || y < 0
            || z < 0
            || x >= self.size_x as i32
            || y >= self.size_y as i32
            || z >= self.size_z as i32
        {
            return 0;
        }
        self.data
            [(x as u32 + y as u32 * self.size_x + z as u32 * self.size_x * self.size_y) as usize]
    }
}
