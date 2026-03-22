use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Instant, SystemTime};

use bevy_ecs::system::{NonSendMut, ResMut};
use tracing::error;

use crate::resources::{
    PrefabEntry, PrefabEntryStatus, PrefabJobKind, PrefabJobResult, PrefabJobSignature,
    PrefabLibrary, PrefabTask, next_prefab_scan_after,
};

pub(crate) fn prefab_sync(mut library: ResMut<PrefabLibrary>, mut task: NonSendMut<PrefabTask>) {
    poll_prefab_job(&mut library, &mut task);

    let now = Instant::now();
    if now >= library.next_scan_at {
        if let Err(err) = rescan_prefab_library(&mut library, task.active_source.as_deref()) {
            error!(
                "[prefab_sync] failed to scan {}: {err}",
                library.source_dir.display()
            );
        }
        library.next_scan_at = next_prefab_scan_after(now);
    }

    if task.pending.is_none() {
        if let Some((source_path, cache_path, signature)) = take_next_prefab_job(&mut library) {
            spawn_prefab_job(source_path, cache_path, signature, &mut task);
        }
    }
}

fn poll_prefab_job(library: &mut PrefabLibrary, task: &mut PrefabTask) {
    let Some(rx) = task.pending.take() else {
        return;
    };

    match rx.try_recv() {
        Ok(result) => {
            task.active_source = None;
            apply_prefab_job_result(library, result);
        }
        Err(mpsc::TryRecvError::Empty) => {
            task.pending = Some(rx);
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            task.active_source = None;
            error!("[prefab_sync] background worker disconnected before sending a result");
        }
    }
}

fn apply_prefab_job_result(library: &mut PrefabLibrary, result: PrefabJobResult) {
    let Some(entry) = library
        .entries
        .iter_mut()
        .find(|entry| entry.source_path == result.source_path)
    else {
        return;
    };

    match result.result {
        Ok(prefab) => {
            let cache_modified = file_modified(&entry.cache_path);
            entry.name = prefab.name.clone();
            entry.cache_resolution = Some(prefab.import_resolution);
            entry.cache_modified = cache_modified;
            entry.loaded_cache_modified = cache_modified;
            entry.prefab = Some(prefab);
            entry.status = PrefabEntryStatus::Ready;
        }
        Err(message) => {
            entry.status = PrefabEntryStatus::Error(message);
        }
    }

    entry.last_attempt = Some(result.signature);
    library.prefab_generation = library.prefab_generation.wrapping_add(1);
    library.ensure_selected_entry();
}

fn rescan_prefab_library(
    library: &mut PrefabLibrary,
    active_source: Option<&Path>,
) -> std::io::Result<()> {
    let fs = capy_assets::OsFileSystem;
    let selected_source = library.selected_source.clone();
    let discovered = scan_prefab_sources(&library.source_dir)?;
    let mut existing = std::mem::take(&mut library.entries)
        .into_iter()
        .map(|entry| (entry.source_path.clone(), entry))
        .collect::<HashMap<_, _>>();
    let mut entries = Vec::with_capacity(discovered.len());

    for source_path in discovered {
        let cache_path = capy_assets::voxel_prefab_cache_path(
            &source_path,
            &library.source_dir,
            &library.cache_dir,
        );
        let mut entry = existing
            .remove(&source_path)
            .unwrap_or_else(|| PrefabEntry::new(source_path.clone(), cache_path.clone()));
        let previous_cache_modified = entry.cache_modified;

        entry.source_path = source_path.clone();
        entry.cache_path = cache_path;
        entry.source_modified = file_modified(&source_path);
        entry.cache_modified = file_modified(&entry.cache_path);
        if previous_cache_modified != entry.cache_modified {
            entry.prefab = None;
            entry.loaded_cache_modified = None;
        }

        let mut metadata_error = None;
        if let Some(cache_modified) = entry.cache_modified {
            if previous_cache_modified != Some(cache_modified) || entry.metadata.is_none() {
                match capy_assets::read_voxel_prefab_metadata(&entry.cache_path, &fs) {
                    Ok(metadata) => {
                        entry.name = metadata.name.clone();
                        entry.cache_resolution = Some(metadata.import_resolution);
                        entry.metadata = Some(metadata);
                    }
                    Err(err) => {
                        entry.metadata = None;
                        entry.cache_resolution = None;
                        metadata_error = Some(err.to_string());
                    }
                }
            }
        } else {
            entry.metadata = None;
            entry.cache_resolution = None;
        }

        if active_source == Some(entry.source_path.as_path()) {
            entries.push(entry);
            continue;
        }

        let is_selected = selected_source.as_deref() == Some(entry.source_path.as_path());
        if let Some((signature, status)) =
            desired_prefab_job(&entry, library.default_resolution, is_selected)
        {
            if entry.last_attempt.as_ref() != Some(&signature) {
                entry.last_attempt = Some(signature);
                entry.status = status;
            }
        } else if let Some(message) = metadata_error {
            entry.status = PrefabEntryStatus::Error(message);
        } else if entry.cache_modified.is_some() && entry.cache_resolution.is_some() {
            entry.status = PrefabEntryStatus::Ready;
        }

        entries.push(entry);
    }

    entries.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.source_path.cmp(&right.source_path))
    });
    library.entries = entries;
    library.ensure_selected_entry();

    Ok(())
}

fn desired_prefab_job(
    entry: &PrefabEntry,
    default_resolution: u32,
    is_selected: bool,
) -> Option<(PrefabJobSignature, PrefabEntryStatus)> {
    if entry.cache_modified.is_none() {
        return Some((
            PrefabJobSignature {
                kind: PrefabJobKind::Regenerate {
                    resolution: default_resolution,
                },
                source_modified: entry.source_modified,
                cache_modified: None,
            },
            PrefabEntryStatus::QueuedRegenerate {
                resolution: default_resolution,
                reason: String::from("missing cache"),
            },
        ));
    }

    if is_selected
        && (entry.prefab.is_none() || entry.loaded_cache_modified != entry.cache_modified)
    {
        return Some((
            PrefabJobSignature {
                kind: PrefabJobKind::LoadCache,
                source_modified: entry.source_modified,
                cache_modified: entry.cache_modified,
            },
            PrefabEntryStatus::QueuedLoad,
        ));
    }

    None
}

fn take_next_prefab_job(
    library: &mut PrefabLibrary,
) -> Option<(PathBuf, PathBuf, PrefabJobSignature)> {
    if let Some(selected_source) = library.selected_source.clone() {
        if let Some(entry) = library
            .entries
            .iter_mut()
            .find(|entry| entry.source_path == selected_source)
        {
            if matches!(entry.status, PrefabEntryStatus::QueuedLoad) {
                let signature = entry.last_attempt.clone()?;
                entry.status = PrefabEntryStatus::LoadingCache;
                return Some((
                    entry.source_path.clone(),
                    entry.cache_path.clone(),
                    signature,
                ));
            }

            if let PrefabEntryStatus::QueuedRegenerate { resolution, reason } = &entry.status {
                let signature = entry.last_attempt.clone()?;
                entry.status = PrefabEntryStatus::Voxelizing {
                    resolution: *resolution,
                    reason: reason.clone(),
                };
                return Some((
                    entry.source_path.clone(),
                    entry.cache_path.clone(),
                    signature,
                ));
            }
        }
    }

    for entry in &mut library.entries {
        if matches!(entry.status, PrefabEntryStatus::QueuedLoad) {
            let signature = entry.last_attempt.clone()?;
            entry.status = PrefabEntryStatus::LoadingCache;
            return Some((
                entry.source_path.clone(),
                entry.cache_path.clone(),
                signature,
            ));
        }
    }

    for entry in &mut library.entries {
        let PrefabEntryStatus::QueuedRegenerate { resolution, reason } = &entry.status else {
            continue;
        };
        let signature = entry.last_attempt.clone()?;
        entry.status = PrefabEntryStatus::Voxelizing {
            resolution: *resolution,
            reason: reason.clone(),
        };
        return Some((
            entry.source_path.clone(),
            entry.cache_path.clone(),
            signature,
        ));
    }

    None
}

fn spawn_prefab_job(
    source_path: PathBuf,
    cache_path: PathBuf,
    signature: PrefabJobSignature,
    task: &mut PrefabTask,
) {
    let worker_source = source_path.clone();
    let worker_signature = signature.clone();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let fs = capy_assets::OsFileSystem;
        let result = match worker_signature.kind {
            PrefabJobKind::LoadCache => {
                capy_assets::load_voxel_prefab(&cache_path, &worker_source, &fs)
                    .map_err(|err| err.to_string())
            }
            PrefabJobKind::Regenerate { resolution } => {
                capy_assets::regenerate_fbx_prefab_cache_to_path(
                    &worker_source,
                    &cache_path,
                    resolution,
                    &fs,
                )
                .map_err(|err| err.to_string())
            }
        };
        let _ = tx.send(PrefabJobResult {
            source_path: worker_source,
            signature: worker_signature,
            result,
        });
    });

    task.active_source = Some(source_path);
    task.pending = Some(rx);
}

fn scan_prefab_sources(root_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    if !root_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    collect_prefab_sources(root_dir, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_prefab_sources(dir: &Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_prefab_sources(&path, paths)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("fbx"))
        {
            paths.push(path);
        }
    }

    Ok(())
}

fn file_modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}
