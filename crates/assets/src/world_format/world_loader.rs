use std::collections::HashMap;
use std::path::Path;

use capy_core::{BakedChunkData, MATERIAL_PALETTE_SIZE, RegionCoord, VoxelMeshData};

use crate::error::{AssetError, Result};

use super::file_system::FileSystem;
use super::region_io;
use super::types::{Compression, RegionEntry, WorldManifest};
use crate::resources::WorldHandle;

const REGION_DIM: i32 = 4;

pub fn load_world_as_mesh_data(world_dir: &Path, fs: &impl FileSystem) -> Result<VoxelMeshData> {
    let manifest = WorldManifest::load(world_dir, fs)?;

    let first_entry =
        manifest
            .regions
            .values()
            .next()
            .ok_or_else(|| AssetError::CorruptRegion {
                reason: "manifest contains no regions".into(),
            })?;

    let region_path = manifest.region_file_path(world_dir, first_entry.coord);
    let chunks = region_io::load_region(
        first_entry.coord,
        &region_path,
        Some(&first_entry.content_hash),
        fs,
    )?;

    let (_key, chunk) = chunks
        .into_iter()
        .next()
        .ok_or_else(|| AssetError::CorruptRegion {
            reason: "region contains no chunks".into(),
        })?;

    let mut palette = [[0.0f32; 3]; MATERIAL_PALETTE_SIZE];
    for (i, color) in manifest
        .material_palette
        .iter()
        .enumerate()
        .take(MATERIAL_PALETTE_SIZE)
    {
        palette[i] = *color;
    }

    Ok(VoxelMeshData::from_single_chunk(
        chunk,
        manifest.chunk_size,
        manifest.chunk_size,
        palette,
    ))
}

pub fn open_world_handle(world_dir: &Path, fs: &impl FileSystem) -> Result<WorldHandle> {
    WorldManifest::load(world_dir, fs)?;
    Ok(WorldHandle::new(world_dir.to_path_buf()))
}

pub fn save_generated_world(
    baked: BakedChunkData,
    chunk_size: u32,
    material_palette: Vec<[f32; 3]>,
    world_dir: &Path,
    fs: &impl FileSystem,
) -> Result<()> {
    let mut chunks = HashMap::new();
    chunks.insert((0u8, 0u8, 0u8), baked);

    let mut manifest = WorldManifest::new(chunk_size, 4, Compression::Lz4, material_palette);

    let coord = RegionCoord { x: 0, y: 0, z: 0 };
    let path = manifest.region_file_path(world_dir, coord);
    let content_hash = region_io::save_region(&path, &chunks, Compression::Lz4, fs)?;

    manifest.regions.insert(
        coord,
        RegionEntry {
            coord,
            content_hash,
        },
    );
    manifest.save(world_dir, fs)?;

    Ok(())
}

/// Save edited chunks grouped into region files.
/// Only edited (non-canonical) chunks are persisted; canonical flat terrain is
/// regenerated deterministically on load.
pub fn save_edited_world(
    edited: &HashMap<[i32; 3], BakedChunkData>,
    chunk_size: u32,
    material_palette: &[[f32; 3]],
    world_dir: &Path,
    fs: &impl FileSystem,
) -> Result<()> {
    let mut manifest = WorldManifest::new(
        chunk_size,
        REGION_DIM as u32,
        Compression::Lz4,
        material_palette.to_vec(),
    );

    // Group edited chunks by region coordinate.
    let mut by_region: HashMap<RegionCoord, HashMap<(u8, u8, u8), BakedChunkData>> = HashMap::new();
    for (coord, baked) in edited {
        let region = RegionCoord {
            x: coord[0].div_euclid(REGION_DIM),
            y: coord[1].div_euclid(REGION_DIM),
            z: coord[2].div_euclid(REGION_DIM),
        };
        let local = (
            coord[0].rem_euclid(REGION_DIM) as u8,
            coord[1].rem_euclid(REGION_DIM) as u8,
            coord[2].rem_euclid(REGION_DIM) as u8,
        );
        by_region
            .entry(region)
            .or_default()
            .insert(local, baked.clone());
    }

    for (region_coord, chunks) in &by_region {
        let path = manifest.region_file_path(world_dir, *region_coord);
        let content_hash = region_io::save_region(&path, chunks, Compression::Lz4, fs)?;
        manifest.regions.insert(
            *region_coord,
            RegionEntry {
                coord: *region_coord,
                content_hash,
            },
        );
    }

    manifest.save(world_dir, fs)?;
    Ok(())
}

/// Load all chunks from a saved world, returning world-space chunk coordinates
/// mapped to their baked data.
pub fn load_world_chunks(
    world_dir: &Path,
    fs: &impl FileSystem,
) -> Result<HashMap<[i32; 3], BakedChunkData>> {
    let manifest = WorldManifest::load(world_dir, fs)?;
    let region_dim = manifest.region_dim as i32;

    let mut all_chunks = HashMap::new();
    for entry in manifest.regions.values() {
        let path = manifest.region_file_path(world_dir, entry.coord);
        let chunks = region_io::load_region(entry.coord, &path, Some(&entry.content_hash), fs)?;
        for ((lx, ly, lz), baked) in chunks {
            let world_coord = [
                entry.coord.x * region_dim + lx as i32,
                entry.coord.y * region_dim + ly as i32,
                entry.coord.z * region_dim + lz as i32,
            ];
            all_chunks.insert(world_coord, baked);
        }
    }

    Ok(all_chunks)
}
