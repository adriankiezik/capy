use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use crate::error::{AssetError, Result};
use crate::prefab_import::{VoxelPrefabAsset, import_fbx_prefab};
use crate::world_format::FileSystem;

const PREFAB_CACHE_MAGIC: [u8; 4] = *b"VPFB";
const PREFAB_CACHE_VERSION_V1: u16 = 1;
const PREFAB_CACHE_VERSION_V2: u16 = 2;
const PREFAB_CACHE_VERSION_CURRENT: u16 = PREFAB_CACHE_VERSION_V2;
const PREFAB_CACHE_PREFIX_LEN: usize = 10;
const PREFAB_CACHE_STATIC_HEADER_LEN: usize = 42;
const MAX_PREFAB_NAME_LEN: usize = 4096;

#[derive(Debug, Clone)]
pub struct VoxelPrefabMetadata {
    pub name: String,
    pub import_resolution: u32,
    pub size: [u32; 3],
    pub anchor: [i32; 3],
    pub filled_voxel_count: usize,
}

pub const DEFAULT_PREFAB_SOURCE_DIR: &str = "assets/prefabs/source";
pub const DEFAULT_PREFAB_CACHE_DIR: &str = "assets/prefabs/cache";
pub const DEFAULT_PREFAB_RESOLUTION: u32 = 128;

pub fn voxel_prefab_cache_path(source_path: &Path, source_dir: &Path, cache_dir: &Path) -> PathBuf {
    match source_path.strip_prefix(source_dir) {
        Ok(relative) => cache_dir.join(relative),
        Err(_) => cache_dir.join(source_path.file_name().unwrap_or(source_path.as_os_str())),
    }
    .with_extension("voxel")
}

pub fn regenerate_fbx_prefab_cache_to_path(
    fbx_path: &Path,
    cache_path: &Path,
    resolution: u32,
    fs: &impl FileSystem,
) -> Result<VoxelPrefabAsset> {
    let prefab = import_fbx_prefab(fbx_path, resolution, fs)?;
    save_voxel_prefab(&prefab, cache_path, fs)?;
    Ok(prefab)
}

pub fn load_voxel_prefab(
    cache_path: &Path,
    source_path: &Path,
    fs: &impl FileSystem,
) -> Result<VoxelPrefabAsset> {
    let bytes = fs.read(cache_path)?;
    let mut reader = Cursor::new(bytes.as_slice());
    let (metadata, version) = read_prefab_metadata(cache_path, &mut reader)?;
    let voxel_count = read_u32(cache_path, &mut reader)? as usize;
    let compressed_len = read_u32(cache_path, &mut reader)? as usize;
    let mut compressed = vec![0u8; compressed_len];
    reader.read_exact(&mut compressed).map_err(AssetError::Io)?;
    let voxel_bytes = lz4_flex::decompress_size_prepended(&compressed).map_err(|err| {
        AssetError::InvalidVoxelCache {
            path: cache_path.to_path_buf(),
            reason: format!("failed to decompress voxel data: {err}"),
        }
    })?;
    let voxels = decode_voxels(cache_path, &voxel_bytes, version)?;

    if voxels.len() != voxel_count {
        return Err(AssetError::InvalidVoxelCache {
            path: cache_path.to_path_buf(),
            reason: format!(
                "voxel count mismatch: header={voxel_count}, actual={}",
                voxels.len()
            ),
        });
    }

    let filled_voxel_count = voxels.iter().filter(|&&material| material != 0).count();
    Ok(VoxelPrefabAsset {
        name: metadata.name,
        source_path: source_path.to_path_buf(),
        import_resolution: metadata.import_resolution,
        size: metadata.size,
        anchor: metadata.anchor,
        filled_voxel_count,
        voxels,
    })
}

pub fn read_voxel_prefab_metadata(
    cache_path: &Path,
    fs: &impl FileSystem,
) -> Result<VoxelPrefabMetadata> {
    let prefix = fs.read_prefix(cache_path, PREFAB_CACHE_PREFIX_LEN)?;
    let mut prefix_reader = Cursor::new(prefix.as_slice());
    let (_, _, name_len) = read_prefab_prefix(cache_path, &mut prefix_reader)?;
    let metadata_len = PREFAB_CACHE_STATIC_HEADER_LEN
        .checked_add(name_len)
        .ok_or_else(|| AssetError::InvalidVoxelCache {
            path: cache_path.to_path_buf(),
            reason: String::from("prefab metadata header length overflow"),
        })?;
    let bytes = fs.read_prefix(cache_path, metadata_len)?;
    let mut reader = Cursor::new(bytes.as_slice());
    Ok(read_prefab_metadata(cache_path, &mut reader)?.0)
}

pub fn save_voxel_prefab(
    prefab: &VoxelPrefabAsset,
    cache_path: &Path,
    fs: &impl FileSystem,
) -> Result<()> {
    if let Some(parent) = cache_path.parent() {
        fs.create_dir_all(parent)?;
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&PREFAB_CACHE_MAGIC);
    bytes.extend_from_slice(&PREFAB_CACHE_VERSION_CURRENT.to_le_bytes());
    write_string(cache_path, &mut bytes, &prefab.name)?;
    bytes.extend_from_slice(&prefab.import_resolution.to_le_bytes());
    for axis in prefab.size {
        bytes.extend_from_slice(&axis.to_le_bytes());
    }
    for axis in prefab.anchor {
        bytes.extend_from_slice(&axis.to_le_bytes());
    }
    let filled_voxel_count: u32 =
        prefab
            .filled_voxel_count
            .try_into()
            .map_err(|_| AssetError::PrefabCacheWriteFailed {
                path: cache_path.to_path_buf(),
                reason: String::from("filled voxel count does not fit in u32"),
            })?;
    bytes.extend_from_slice(&filled_voxel_count.to_le_bytes());

    let voxel_count: u32 =
        prefab
            .voxels
            .len()
            .try_into()
            .map_err(|_| AssetError::PrefabCacheWriteFailed {
                path: cache_path.to_path_buf(),
                reason: String::from("voxel data length does not fit in u32"),
            })?;
    bytes.extend_from_slice(&voxel_count.to_le_bytes());

    let voxel_bytes = encode_voxels(&prefab.voxels);
    let compressed = lz4_flex::compress_prepend_size(voxel_bytes);
    let compressed_len: u32 =
        compressed
            .len()
            .try_into()
            .map_err(|_| AssetError::PrefabCacheWriteFailed {
                path: cache_path.to_path_buf(),
                reason: String::from("compressed voxel data length does not fit in u32"),
            })?;
    bytes.extend_from_slice(&compressed_len.to_le_bytes());
    bytes.extend_from_slice(&compressed);
    fs.write(cache_path, &bytes)?;

    Ok(())
}

fn read_prefab_metadata(
    cache_path: &Path,
    reader: &mut impl Read,
) -> Result<(VoxelPrefabMetadata, u16)> {
    let (_, version, name_len) = read_prefab_prefix(cache_path, reader)?;
    let name = read_string_with_len(cache_path, reader, name_len)?;
    let import_resolution = read_u32(cache_path, reader)?;
    let size = [
        read_u32(cache_path, reader)?,
        read_u32(cache_path, reader)?,
        read_u32(cache_path, reader)?,
    ];
    let anchor = [
        read_i32(cache_path, reader)?,
        read_i32(cache_path, reader)?,
        read_i32(cache_path, reader)?,
    ];
    let filled_voxel_count = read_u32(cache_path, reader)? as usize;

    Ok((
        VoxelPrefabMetadata {
            name,
            import_resolution,
            size,
            anchor,
            filled_voxel_count,
        },
        version,
    ))
}

fn read_prefab_prefix(cache_path: &Path, reader: &mut impl Read) -> Result<([u8; 4], u16, usize)> {
    let magic = read_bytes::<4>(cache_path, reader)?;
    if magic != PREFAB_CACHE_MAGIC {
        return Err(AssetError::InvalidVoxelCache {
            path: cache_path.to_path_buf(),
            reason: format!("invalid cache magic: {magic:?}"),
        });
    }

    let version = read_u16(cache_path, reader)?;
    if version != PREFAB_CACHE_VERSION_V1 && version != PREFAB_CACHE_VERSION_V2 {
        return Err(AssetError::InvalidVoxelCache {
            path: cache_path.to_path_buf(),
            reason: format!("unsupported cache version: {version}"),
        });
    }

    let name_len = read_u32(cache_path, reader)? as usize;
    if name_len > MAX_PREFAB_NAME_LEN {
        return Err(AssetError::InvalidVoxelCache {
            path: cache_path.to_path_buf(),
            reason: format!("prefab name length {name_len} exceeds {MAX_PREFAB_NAME_LEN}"),
        });
    }

    Ok((magic, version, name_len))
}

fn read_bytes<const N: usize>(cache_path: &Path, reader: &mut impl Read) -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    reader
        .read_exact(&mut buf)
        .map_err(|err| invalid_cache(cache_path, err))?;
    Ok(buf)
}

fn read_u16(cache_path: &Path, reader: &mut impl Read) -> Result<u16> {
    Ok(u16::from_le_bytes(read_bytes(cache_path, reader)?))
}

fn read_u32(cache_path: &Path, reader: &mut impl Read) -> Result<u32> {
    Ok(u32::from_le_bytes(read_bytes(cache_path, reader)?))
}

fn read_i32(cache_path: &Path, reader: &mut impl Read) -> Result<i32> {
    Ok(i32::from_le_bytes(read_bytes(cache_path, reader)?))
}

fn read_string_with_len(cache_path: &Path, reader: &mut impl Read, len: usize) -> Result<String> {
    if len > MAX_PREFAB_NAME_LEN {
        return Err(AssetError::InvalidVoxelCache {
            path: cache_path.to_path_buf(),
            reason: format!("prefab name length {len} exceeds {MAX_PREFAB_NAME_LEN}"),
        });
    }
    let mut bytes = vec![0u8; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|err| invalid_cache(cache_path, err))?;
    String::from_utf8(bytes).map_err(|err| AssetError::InvalidVoxelCache {
        path: cache_path.to_path_buf(),
        reason: format!("invalid UTF-8 in prefab name: {err}"),
    })
}

fn write_string(cache_path: &Path, bytes: &mut Vec<u8>, value: &str) -> Result<()> {
    if value.len() > MAX_PREFAB_NAME_LEN {
        return Err(AssetError::PrefabCacheWriteFailed {
            path: cache_path.to_path_buf(),
            reason: format!("prefab name length exceeds {MAX_PREFAB_NAME_LEN} bytes"),
        });
    }
    let len: u32 = value
        .len()
        .try_into()
        .map_err(|_| AssetError::PrefabCacheWriteFailed {
            path: cache_path.to_path_buf(),
            reason: String::from("prefab name length does not fit in u32"),
        })?;
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn invalid_cache(cache_path: &Path, err: std::io::Error) -> AssetError {
    AssetError::InvalidVoxelCache {
        path: cache_path.to_path_buf(),
        reason: err.to_string(),
    }
}

fn encode_voxels(voxels: &[capy_core::MaterialId]) -> &[u8] {
    bytemuck::cast_slice(voxels)
}

fn decode_voxels(
    cache_path: &Path,
    data: &[u8],
    version: u16,
) -> Result<Vec<capy_core::MaterialId>> {
    match version {
        PREFAB_CACHE_VERSION_V1 => Ok(data
            .iter()
            .map(|&voxel| voxel as capy_core::MaterialId)
            .collect()),
        PREFAB_CACHE_VERSION_V2 => {
            if !data.len().is_multiple_of(2) {
                return Err(AssetError::InvalidVoxelCache {
                    path: cache_path.to_path_buf(),
                    reason: String::from("voxel byte data length is not aligned to u16"),
                });
            }

            let mut voxels = vec![0u16; data.len() / 2];
            let dst: &mut [u8] = bytemuck::cast_slice_mut(&mut voxels);
            dst.copy_from_slice(data);
            Ok(voxels)
        }
        _ => Err(AssetError::InvalidVoxelCache {
            path: cache_path.to_path_buf(),
            reason: format!("unsupported cache version: {version}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::io;
    use std::path::{Path, PathBuf};

    use super::{
        PREFAB_CACHE_MAGIC, PREFAB_CACHE_VERSION_V1, load_voxel_prefab, read_voxel_prefab_metadata,
        save_voxel_prefab,
    };
    use crate::prefab_import::VoxelPrefabAsset;
    use crate::world_format::FileSystem;

    #[derive(Default)]
    struct MemoryFileSystem {
        files: HashMap<PathBuf, Vec<u8>>,
        directories: HashSet<PathBuf>,
    }

    impl FileSystem for MemoryFileSystem {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing file"))
        }

        fn write(&self, _path: &Path, _data: &[u8]) -> io::Result<()> {
            Err(io::Error::other("immutable filesystem"))
        }

        fn create_dir_all(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path) || self.directories.contains(path)
        }
    }

    impl MemoryFileSystem {
        fn write_file(&mut self, path: &Path, data: Vec<u8>) {
            self.files.insert(path.to_path_buf(), data);
        }
    }

    #[test]
    fn voxel_cache_roundtrip_preserves_prefab_data() {
        let cache_path = Path::new("assets/prefabs/tree.voxel");
        let source_path = Path::new("assets/prefabs/tree.fbx");
        let prefab = VoxelPrefabAsset {
            name: String::from("Tree"),
            source_path: source_path.to_path_buf(),
            import_resolution: 128,
            size: [3, 4, 5],
            anchor: [1, 0, 2],
            filled_voxel_count: 3,
            voxels: vec![
                0, 1, 0, 0, 2, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0,
            ],
        };

        let writer = TempWriteFileSystem::default();
        let save_result = save_voxel_prefab(&prefab, cache_path, &writer);
        assert!(save_result.is_ok());

        let mut reader = MemoryFileSystem::default();
        let saved = writer.take_file(cache_path);
        assert!(saved.is_some());
        reader.write_file(cache_path, saved.unwrap_or_default());

        let metadata = read_voxel_prefab_metadata(cache_path, &reader);
        assert!(metadata.is_ok());
        let metadata = metadata.unwrap_or_else(|_| unreachable!());
        assert_eq!(metadata.name, "Tree");
        assert_eq!(metadata.import_resolution, 128);
        assert_eq!(metadata.size, [3, 4, 5]);
        assert_eq!(metadata.anchor, [1, 0, 2]);
        assert_eq!(metadata.filled_voxel_count, 3);

        let loaded = load_voxel_prefab(cache_path, source_path, &reader);
        assert!(loaded.is_ok());
        let loaded = loaded.unwrap_or_else(|_| unreachable!());
        assert_eq!(loaded.name, prefab.name);
        assert_eq!(loaded.source_path, prefab.source_path);
        assert_eq!(loaded.import_resolution, prefab.import_resolution);
        assert_eq!(loaded.size, prefab.size);
        assert_eq!(loaded.anchor, prefab.anchor);
        assert_eq!(loaded.filled_voxel_count, prefab.filled_voxel_count);
        assert_eq!(loaded.voxels, prefab.voxels);
    }

    #[test]
    fn loads_legacy_v1_prefab_cache() {
        let cache_path = Path::new("assets/prefabs/tree_legacy.voxel");
        let source_path = Path::new("assets/prefabs/tree_legacy.fbx");
        let voxels = vec![0u8, 1, 0, 2, 3, 0, 0, 4];

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&PREFAB_CACHE_MAGIC);
        bytes.extend_from_slice(&PREFAB_CACHE_VERSION_V1.to_le_bytes());
        let name = "LegacyTree";
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(&64u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&(voxels.len() as u32).to_le_bytes());
        let compressed = lz4_flex::compress_prepend_size(&voxels);
        bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&compressed);

        let mut reader = MemoryFileSystem::default();
        reader.write_file(cache_path, bytes);

        let metadata = read_voxel_prefab_metadata(cache_path, &reader);
        assert!(metadata.is_ok());
        let metadata = metadata.unwrap_or_else(|_| unreachable!());
        assert_eq!(metadata.name, "LegacyTree");
        assert_eq!(metadata.import_resolution, 64);
        assert_eq!(metadata.size, [2, 2, 2]);
        assert_eq!(metadata.filled_voxel_count, 4);

        let loaded = load_voxel_prefab(cache_path, source_path, &reader);
        assert!(loaded.is_ok());
        let loaded = loaded.unwrap_or_else(|_| unreachable!());
        assert_eq!(loaded.name, "LegacyTree");
        assert_eq!(loaded.source_path, source_path);
        assert_eq!(loaded.import_resolution, 64);
        assert_eq!(loaded.size, [2, 2, 2]);
        assert_eq!(loaded.voxels, vec![0u16, 1, 0, 2, 3, 0, 0, 4]);
    }

    #[derive(Default)]
    struct TempWriteFileSystem {
        files: std::sync::Mutex<HashMap<PathBuf, Vec<u8>>>,
    }

    impl TempWriteFileSystem {
        fn take_file(&self, path: &Path) -> Option<Vec<u8>> {
            self.files.lock().ok()?.remove(path)
        }
    }

    impl FileSystem for TempWriteFileSystem {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.files
                .lock()
                .map_err(|_| io::Error::other("poisoned lock"))?
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing file"))
        }

        fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
            self.files
                .lock()
                .map_err(|_| io::Error::other("poisoned lock"))?
                .insert(path.to_path_buf(), data.to_vec());
            Ok(())
        }

        fn create_dir_all(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }

        fn exists(&self, path: &Path) -> bool {
            self.files
                .lock()
                .ok()
                .is_some_and(|files| files.contains_key(path))
        }
    }
}
