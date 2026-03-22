use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use bevy_ecs::resource::Resource;
use capy_assets::{
    DEFAULT_PREFAB_CACHE_DIR, DEFAULT_PREFAB_RESOLUTION, DEFAULT_PREFAB_SOURCE_DIR,
    VoxelPrefabAsset, VoxelPrefabMetadata,
};
const PREFAB_SCAN_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Resource)]
pub struct PrefabLibrary {
    pub source_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub default_resolution: u32,
    pub regenerate_resolution: u32,
    pub entries: Vec<PrefabEntry>,
    pub selected_source: Option<PathBuf>,
    pub next_scan_at: Instant,
    /// Monotonic counter incremented whenever a prefab job completes.
    pub prefab_generation: u32,
}

#[derive(Debug, Clone)]
pub struct PrefabEntry {
    pub source_path: PathBuf,
    pub cache_path: PathBuf,
    pub name: String,
    pub metadata: Option<VoxelPrefabMetadata>,
    pub prefab: Option<VoxelPrefabAsset>,
    pub cache_resolution: Option<u32>,
    pub source_modified: Option<SystemTime>,
    pub cache_modified: Option<SystemTime>,
    pub loaded_cache_modified: Option<SystemTime>,
    pub status: PrefabEntryStatus,
    pub last_attempt: Option<PrefabJobSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefabEntryStatus {
    QueuedLoad,
    QueuedRegenerate { resolution: u32, reason: String },
    LoadingCache,
    Voxelizing { resolution: u32, reason: String },
    Ready,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefabJobKind {
    LoadCache,
    Regenerate { resolution: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabJobSignature {
    pub kind: PrefabJobKind,
    pub source_modified: Option<SystemTime>,
    pub cache_modified: Option<SystemTime>,
}

impl PrefabLibrary {
    pub fn selected_entry(&self) -> Option<&PrefabEntry> {
        let selected = self.selected_source.as_deref()?;
        self.entries
            .iter()
            .find(|entry| entry.source_path == selected)
    }

    pub fn selected_entry_mut(&mut self) -> Option<&mut PrefabEntry> {
        let selected = self.selected_source.clone()?;
        self.entries
            .iter_mut()
            .find(|entry| entry.source_path == selected)
    }

    pub fn selected_prefab(&self) -> Option<&VoxelPrefabAsset> {
        self.selected_entry()
            .and_then(|entry| entry.prefab.as_ref())
    }

    pub fn set_selected_source(&mut self, source_path: PathBuf) {
        self.selected_source = Some(source_path);
    }

    pub fn selected_is(&self, source_path: &PathBuf) -> bool {
        self.selected_source.as_ref() == Some(source_path)
    }

    pub fn ensure_selected_entry(&mut self) {
        if self.selected_source.is_none() {
            return;
        }

        if self.selected_source.as_ref().is_some_and(|selected| {
            self.entries
                .iter()
                .any(|entry| &entry.source_path == selected)
        }) {
            return;
        }

        self.selected_source = self.entries.first().map(|entry| entry.source_path.clone());
    }

    pub fn ready_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry.status, PrefabEntryStatus::Ready))
            .count()
    }
}

impl PrefabEntry {
    pub fn new(source_path: PathBuf, cache_path: PathBuf) -> Self {
        let name = source_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(String::from)
            .unwrap_or_else(|| String::from("prefab"));

        Self {
            source_path,
            cache_path,
            name,
            metadata: None,
            prefab: None,
            cache_resolution: None,
            source_modified: None,
            cache_modified: None,
            loaded_cache_modified: None,
            status: PrefabEntryStatus::QueuedRegenerate {
                resolution: DEFAULT_PREFAB_RESOLUTION,
                reason: String::from("missing cache"),
            },
            last_attempt: None,
        }
    }

    pub fn status_label(&self) -> String {
        match &self.status {
            PrefabEntryStatus::QueuedLoad => String::from("Queued"),
            PrefabEntryStatus::QueuedRegenerate { resolution, reason } => {
                format!("Queued {resolution} ({reason})")
            }
            PrefabEntryStatus::LoadingCache => String::from("Loading cache"),
            PrefabEntryStatus::Voxelizing { resolution, reason } => {
                format!("Voxelizing {resolution} ({reason})")
            }
            PrefabEntryStatus::Ready => {
                let resolution = self
                    .metadata
                    .as_ref()
                    .map(|metadata| metadata.import_resolution)
                    .or_else(|| self.prefab.as_ref().map(|prefab| prefab.import_resolution))
                    .or(self.cache_resolution)
                    .unwrap_or(DEFAULT_PREFAB_RESOLUTION);
                format!("Ready {resolution}")
            }
            PrefabEntryStatus::Error(message) => format!("Error: {message}"),
        }
    }

    pub fn display_resolution(&self) -> u32 {
        self.metadata
            .as_ref()
            .map(|metadata| metadata.import_resolution)
            .or_else(|| self.prefab.as_ref().map(|prefab| prefab.import_resolution))
            .or(self.cache_resolution)
            .unwrap_or(DEFAULT_PREFAB_RESOLUTION)
    }

    pub fn queue_regenerate(&mut self, resolution: u32, reason: impl Into<String>) {
        self.last_attempt = Some(PrefabJobSignature {
            kind: PrefabJobKind::Regenerate { resolution },
            source_modified: self.source_modified,
            cache_modified: self.cache_modified,
        });
        self.status = PrefabEntryStatus::QueuedRegenerate {
            resolution,
            reason: reason.into(),
        };
    }
}

impl Default for PrefabLibrary {
    fn default() -> Self {
        Self {
            source_dir: PathBuf::from(DEFAULT_PREFAB_SOURCE_DIR),
            cache_dir: PathBuf::from(DEFAULT_PREFAB_CACHE_DIR),
            default_resolution: DEFAULT_PREFAB_RESOLUTION,
            regenerate_resolution: DEFAULT_PREFAB_RESOLUTION,
            entries: Vec::new(),
            selected_source: None,
            next_scan_at: Instant::now(),
            prefab_generation: 0,
        }
    }
}

pub fn next_prefab_scan_after(now: Instant) -> Instant {
    now + PREFAB_SCAN_INTERVAL
}
