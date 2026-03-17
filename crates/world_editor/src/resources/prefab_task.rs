use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use super::prefab_library::PrefabJobSignature;

pub(crate) struct PrefabJobResult {
    pub(crate) source_path: PathBuf,
    pub(crate) signature: PrefabJobSignature,
    pub(crate) result: Result<capy_assets::VoxelPrefabAsset, String>,
}

#[derive(Default)]
pub(crate) struct PrefabTask {
    pub(crate) pending: Option<Receiver<PrefabJobResult>>,
    pub(crate) active_source: Option<PathBuf>,
}
