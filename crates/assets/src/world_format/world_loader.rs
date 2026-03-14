use std::collections::HashMap;
use std::path::Path;

use capy_core::{BakedChunkData, RegionCoord, VoxelMeshData};

use crate::error::{AssetError, Result};

use super::WorldHandle;
use super::region_io;
use super::types::{Compression, RegionEntry, WorldManifest};

pub fn load_world_as_mesh_data(world_dir: &Path) -> Result<VoxelMeshData> {
    let manifest = WorldManifest::load(world_dir)?;

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
    )?;

    let (_key, chunk) = chunks
        .into_iter()
        .next()
        .ok_or_else(|| AssetError::CorruptRegion {
            reason: "region contains no chunks".into(),
        })?;

    let mut palette = [[0.0f32; 3]; 8];
    for (i, color) in manifest.material_palette.iter().enumerate().take(8) {
        palette[i] = *color;
    }

    Ok(VoxelMeshData {
        dag_buffer: chunk.dag_buffer,
        avg_color_buffer: chunk.avg_color_buffer,
        world_size: chunk.world_size,
        root_offset: chunk.root_offset,
        depth: chunk.depth,
        chunk_size: manifest.chunk_size,
        material_palette: palette,
    })
}

pub fn open_world_handle(world_dir: &Path) -> Result<WorldHandle> {
    WorldManifest::load(world_dir)?;
    Ok(WorldHandle::new(world_dir.to_path_buf()))
}

pub fn save_generated_world(
    baked: BakedChunkData,
    chunk_size: u32,
    material_palette: Vec<[f32; 3]>,
    world_dir: &Path,
) -> Result<()> {
    let mut chunks = HashMap::new();
    chunks.insert((0u8, 0u8, 0u8), baked);

    let mut manifest = WorldManifest::new(chunk_size, 4, Compression::Lz4, material_palette);

    let coord = RegionCoord { x: 0, y: 0, z: 0 };
    let path = manifest.region_file_path(world_dir, coord);
    let content_hash = region_io::save_region(&path, &chunks, Compression::Lz4)?;

    manifest.regions.insert(
        coord,
        RegionEntry {
            coord,
            content_hash,
        },
    );
    manifest.save(world_dir)?;

    Ok(())
}
