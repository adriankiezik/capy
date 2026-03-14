use std::collections::HashMap;

use capy_core::RegionCoord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Compression {
    None = 0,
    Lz4 = 1,
}

impl Compression {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::None),
            1 => Some(Self::Lz4),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegionEntry {
    pub coord: RegionCoord,
    pub content_hash: [u8; 16],
}

#[derive(Debug, Clone)]
pub struct WorldManifest {
    pub format_version: u16,
    pub compression: Compression,
    pub chunk_size: u32,
    pub region_dim: u32,
    pub material_palette: Vec<[f32; 3]>,
    pub regions: HashMap<RegionCoord, RegionEntry>,
}
