use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use capy_core::{BakedChunkData, RegionCoord};

use crate::error::{AssetError, Result};

use super::binary_io::{read_bytes, read_u8, read_u16_le, read_u32_le};
use super::file_system::FileSystem;
use super::hash;
use super::types::{Compression, CompressionCodec};

const MAGIC: [u8; 4] = *b"WREG";
const FORMAT_VERSION: u16 = 1;

const CHUNK_HEADER_SIZE: usize = 32;
const FILE_HEADER_SIZE: usize = 16;

pub fn load_region(
    coord: RegionCoord,
    path: &Path,
    expected_hash: Option<&[u8; 16]>,
    fs: &impl FileSystem,
) -> Result<HashMap<(u8, u8, u8), BakedChunkData>> {
    if !fs.exists(path) {
        return Err(AssetError::RegionNotFound(path.to_path_buf()));
    }

    let file_data = fs.read(path)?;

    if let Some(expected) = expected_hash {
        let actual = hash::content_hash(&file_data);
        if actual != *expected {
            return Err(AssetError::HashMismatch {
                rx: coord.x,
                ry: coord.y,
                rz: coord.z,
            });
        }
    }

    if file_data.len() < FILE_HEADER_SIZE {
        return Err(AssetError::CorruptRegion {
            reason: "file too small for header".into(),
        });
    }

    let mut header = Cursor::new(&file_data[..FILE_HEADER_SIZE]);

    let magic = read_bytes::<4>(&mut header)?;
    if magic != MAGIC {
        return Err(AssetError::InvalidMagic {
            expected: MAGIC,
            actual: magic,
        });
    }

    let version = read_u16_le(&mut header)?;
    if version != FORMAT_VERSION {
        return Err(AssetError::UnsupportedVersion(version));
    }

    let compression_id = read_u8(&mut header)?;
    let compression = Compression::from_u8(compression_id)
        .ok_or(AssetError::UnsupportedCompression(compression_id))?;

    let chunk_count = read_u8(&mut header)? as usize;
    let _uncompressed_size = read_u32_le(&mut header)?;
    let _reserved = read_u32_le(&mut header)?;

    let compressed_payload = &file_data[FILE_HEADER_SIZE..];
    let payload = compression.decompress(compressed_payload)?;

    if payload.len() < chunk_count * CHUNK_HEADER_SIZE {
        return Err(AssetError::CorruptRegion {
            reason: "payload too small for chunk headers".into(),
        });
    }

    let mut chunks = HashMap::with_capacity(chunk_count);
    let directory_end = chunk_count * CHUNK_HEADER_SIZE;

    for i in 0..chunk_count {
        let offset = i * CHUNK_HEADER_SIZE;
        let mut r = Cursor::new(&payload[offset..offset + CHUNK_HEADER_SIZE]);

        let cx = read_u8(&mut r)?;
        let cy = read_u8(&mut r)?;
        let cz = read_u8(&mut r)?;
        let _pad = read_u8(&mut r)?;
        let dag_offset = read_u32_le(&mut r)? as usize;
        let dag_word_count = read_u32_le(&mut r)? as usize;
        let avg_color_offset = read_u32_le(&mut r)? as usize;
        let avg_color_word_count = read_u32_le(&mut r)? as usize;
        let root_offset = read_u32_le(&mut r)?;
        let world_size = read_u32_le(&mut r)?;
        let depth = read_u32_le(&mut r)?;

        let dag_start = directory_end + dag_offset;
        let dag_byte_count = dag_word_count * 4;
        let avg_start = directory_end + avg_color_offset;
        let avg_byte_count = avg_color_word_count * 4;

        if dag_start + dag_byte_count > payload.len() || avg_start + avg_byte_count > payload.len()
        {
            return Err(AssetError::CorruptRegion {
                reason: format!("chunk ({cx},{cy},{cz}) data extends beyond payload"),
            });
        }

        let dag_buffer = bytes_to_u32_vec(&payload[dag_start..dag_start + dag_byte_count]);
        let avg_color_buffer = bytes_to_u32_vec(&payload[avg_start..avg_start + avg_byte_count]);

        chunks.insert(
            (cx, cy, cz),
            BakedChunkData {
                dag_buffer,
                avg_color_buffer,
                root_offset,
                world_size,
                depth,
            },
        );
    }

    Ok(chunks)
}

pub fn save_region(
    path: &Path,
    chunks: &HashMap<(u8, u8, u8), BakedChunkData>,
    compression: Compression,
    fs: &impl FileSystem,
) -> Result<[u8; 16]> {
    if let Some(parent) = path.parent() {
        fs.create_dir_all(parent)?;
    }

    let chunk_count = chunks.len();
    if chunk_count > u8::MAX as usize {
        return Err(AssetError::TooManyChunksInRegion {
            count: chunk_count,
            max: u8::MAX as usize,
        });
    }

    let mut dag_section = Vec::new();
    let mut avg_section = Vec::new();
    let mut headers = Vec::with_capacity(chunk_count * CHUNK_HEADER_SIZE);

    let mut sorted_keys: Vec<_> = chunks.keys().copied().collect();
    sorted_keys.sort();
    let total_dag_section_size: usize = sorted_keys
        .iter()
        .map(|key| chunks[key].dag_buffer.len() * std::mem::size_of::<u32>())
        .sum();

    for &(cx, cy, cz) in &sorted_keys {
        let chunk = &chunks[&(cx, cy, cz)];

        let dag_offset = dag_section.len() as u32;
        let dag_word_count = chunk.dag_buffer.len() as u32;
        let avg_color_offset = (total_dag_section_size + avg_section.len()) as u32;
        let avg_color_word_count = chunk.avg_color_buffer.len() as u32;

        headers.push(cx);
        headers.push(cy);
        headers.push(cz);
        headers.push(0u8);
        headers.extend_from_slice(&dag_offset.to_le_bytes());
        headers.extend_from_slice(&dag_word_count.to_le_bytes());
        headers.extend_from_slice(&avg_color_offset.to_le_bytes());
        headers.extend_from_slice(&avg_color_word_count.to_le_bytes());
        headers.extend_from_slice(&chunk.root_offset.to_le_bytes());
        headers.extend_from_slice(&chunk.world_size.to_le_bytes());
        headers.extend_from_slice(&chunk.depth.to_le_bytes());

        append_u32_slice_as_le_bytes(&mut dag_section, &chunk.dag_buffer);
        append_u32_slice_as_le_bytes(&mut avg_section, &chunk.avg_color_buffer);
    }

    let mut payload = Vec::with_capacity(headers.len() + dag_section.len() + avg_section.len());
    payload.extend_from_slice(&headers);
    payload.extend_from_slice(&dag_section);
    payload.extend_from_slice(&avg_section);

    let uncompressed_size = payload.len() as u32;

    let compressed = compression.compress(&payload);

    let mut file_data = Vec::with_capacity(FILE_HEADER_SIZE + compressed.len());
    file_data.extend_from_slice(&MAGIC);
    file_data.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    file_data.push(compression as u8);
    file_data.push(chunk_count as u8);
    file_data.extend_from_slice(&uncompressed_size.to_le_bytes());
    file_data.extend_from_slice(&0u32.to_le_bytes());
    file_data.extend_from_slice(&compressed);

    fs.write(path, &file_data)?;

    Ok(hash::content_hash(&file_data))
}

fn bytes_to_u32_vec(data: &[u8]) -> Vec<u32> {
    data.chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn append_u32_slice_as_le_bytes(output: &mut Vec<u8>, data: &[u32]) {
    output.reserve(data.len() * std::mem::size_of::<u32>());
    for value in data {
        output.extend_from_slice(&value.to_le_bytes());
    }
}
