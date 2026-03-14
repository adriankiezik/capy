use std::collections::HashMap;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};

use capy_core::RegionCoord;

use crate::error::{AssetError, Result};

use super::binary_io::{read_bytes, read_u8, read_u16_le, read_u32_le};
use super::types::{Compression, RegionEntry, WorldManifest};

const MAGIC: [u8; 4] = *b"CAPY";
const FORMAT_VERSION: u16 = 1;

impl WorldManifest {
    pub fn new(
        chunk_size: u32,
        region_dim: u32,
        compression: Compression,
        material_palette: Vec<[f32; 3]>,
    ) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            compression,
            chunk_size,
            region_dim,
            material_palette,
            regions: HashMap::new(),
        }
    }

    pub fn load(world_dir: &Path) -> Result<Self> {
        let path = world_dir.join("world.manifest");
        if !path.exists() {
            return Err(AssetError::ManifestNotFound(path));
        }
        let data = std::fs::read(&path)?;
        Self::from_bytes(&data)
    }

    pub fn save(&self, world_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(world_dir)?;
        let path = world_dir.join("world.manifest");
        let data = self.to_bytes();
        std::fs::write(&path, &data)?;
        Ok(())
    }

    pub fn region_file_path(&self, world_dir: &Path, coord: RegionCoord) -> PathBuf {
        world_dir
            .join("regions")
            .join(format!("r_{}_{}_{}.world", coord.x, coord.y, coord.z))
    }

    fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut r = Cursor::new(data);

        let magic = read_bytes::<4>(&mut r)?;
        if magic != MAGIC {
            return Err(AssetError::InvalidMagic {
                expected: MAGIC,
                actual: magic,
            });
        }

        let version = read_u16_le(&mut r)?;
        if version != FORMAT_VERSION {
            return Err(AssetError::UnsupportedVersion(version));
        }

        let compression_id = read_u8(&mut r)?;
        let compression = Compression::from_u8(compression_id)
            .ok_or(AssetError::UnsupportedCompression(compression_id))?;

        let _padding = read_u8(&mut r)?;

        let chunk_size = read_u32_le(&mut r)?;
        let region_dim = read_u32_le(&mut r)?;

        let material_count = read_u8(&mut r)? as usize;
        let _pad = read_bytes::<3>(&mut r)?;

        let mut material_palette = Vec::with_capacity(material_count);
        for _ in 0..material_count {
            let r_val = read_f32_le(&mut r)?;
            let g_val = read_f32_le(&mut r)?;
            let b_val = read_f32_le(&mut r)?;
            material_palette.push([r_val, g_val, b_val]);
        }

        let region_count = read_u32_le(&mut r)? as usize;
        let mut regions = HashMap::with_capacity(region_count);
        for _ in 0..region_count {
            let x = read_i32_le(&mut r)?;
            let y = read_i32_le(&mut r)?;
            let z = read_i32_le(&mut r)?;
            let hash = read_bytes::<16>(&mut r)?;
            let coord = RegionCoord { x, y, z };
            regions.insert(
                coord,
                RegionEntry {
                    coord,
                    content_hash: hash,
                },
            );
        }

        Ok(Self {
            format_version: version,
            compression,
            chunk_size,
            region_dim,
            material_palette,
            regions,
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut w = Vec::new();

        write_all(&mut w, &MAGIC);
        write_u16_le(&mut w, self.format_version);
        write_u8(&mut w, self.compression as u8);
        write_u8(&mut w, 0);

        write_u32_le(&mut w, self.chunk_size);
        write_u32_le(&mut w, self.region_dim);

        let mat_count = self.material_palette.len().min(255) as u8;
        write_u8(&mut w, mat_count);
        write_all(&mut w, &[0u8; 3]);

        for color in &self.material_palette[..mat_count as usize] {
            write_f32_le(&mut w, color[0]);
            write_f32_le(&mut w, color[1]);
            write_f32_le(&mut w, color[2]);
        }

        write_u32_le(&mut w, self.regions.len() as u32);
        let mut entries: Vec<_> = self.regions.values().collect();
        entries.sort_by_key(|entry| (entry.coord.x, entry.coord.y, entry.coord.z));
        for entry in entries {
            write_i32_le(&mut w, entry.coord.x);
            write_i32_le(&mut w, entry.coord.y);
            write_i32_le(&mut w, entry.coord.z);
            write_all(&mut w, &entry.content_hash);
        }

        w
    }
}

fn read_i32_le(r: &mut impl Read) -> io::Result<i32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

fn read_f32_le(r: &mut impl Read) -> io::Result<f32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

fn write_all(w: &mut Vec<u8>, data: &[u8]) {
    w.extend_from_slice(data);
}

fn write_u8(w: &mut Vec<u8>, v: u8) {
    w.push(v);
}

fn write_u16_le(w: &mut Vec<u8>, v: u16) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn write_u32_le(w: &mut Vec<u8>, v: u32) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn write_i32_le(w: &mut Vec<u8>, v: i32) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn write_f32_le(w: &mut Vec<u8>, v: f32) {
    w.extend_from_slice(&v.to_le_bytes());
}
