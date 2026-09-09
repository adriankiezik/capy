use crate::error::{Result, StyleError};
use crate::workspace::Workspace;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn affected(changes: &Path, workspace: &Workspace) -> Result<Vec<PathBuf>> {
    let root = workspace.root();

    let mut directories = workspace.directories()?;

    directories.sort_by_key(|directory| std::cmp::Reverse(directory.components().count()));

    let tool = Path::new(env!("CARGO_MANIFEST_DIR")).strip_prefix(root)?;

    let contents = fs::read(changes).map_err(|source| StyleError::Io {
        operation: "Reading staged paths",
        source,
    })?;

    let mut selected = BTreeSet::new();

    for path in contents
        .split(|&byte| byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = Path::new(std::str::from_utf8(path)?);

        if path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(StyleError::StagedPath);
        }

        let shared = matches!(
            path.to_str(),
            Some(
                "Cargo.toml"
                    | "Cargo.lock"
                    | "engine/Cargo.toml"
                    | "engine/Cargo.lock"
                    | "game/Cargo.toml"
                    | "game/Cargo.lock"
                    | "rustfmt.toml"
                    | ".rustfmt.toml"
                    | "rust-toolchain"
                    | "rust-toolchain.toml"
            )
        ) || path.starts_with(".cargo")
            || path.starts_with(".githooks")
            || path.starts_with(tool);

        if shared {
            selected.extend(directories.iter().map(|directory| root.join(directory)));
        } else if let Some(directory) = directories
            .iter()
            .find(|directory| path.starts_with(directory))
        {
            selected.insert(root.join(directory));
        }
    }

    Ok(selected.into_iter().collect())
}
