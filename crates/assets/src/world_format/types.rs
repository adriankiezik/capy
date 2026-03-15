use std::collections::HashMap;

use capy_core::RegionCoord;

use crate::error::Result;

use super::codec;

pub trait CompressionCodec {
    fn compress(&self, data: &[u8]) -> Vec<u8>;
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>>;
}

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

impl CompressionCodec for Compression {
    fn compress(&self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::None => data.to_vec(),
            Self::Lz4 => codec::compress_lz4(data),
        }
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self {
            Self::None => Ok(data.to_vec()),
            Self::Lz4 => codec::decompress_lz4(data),
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
