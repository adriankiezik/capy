use crate::error::{Result, StyleError};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Workspace {
    root: PathBuf,
    packages: Vec<Value>,
}

pub struct SourceTarget {
    pub path: PathBuf,
    pub crate_name: Option<String>,
}

impl Workspace {
    pub fn discover() -> Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or(StyleError::MissingMetadata("repository root"))?
            .to_owned();

        let mut packages = Vec::new();

        for workspace in ["engine", "game"] {
            let output = Command::new("cargo")
                .args([
                    "metadata",
                    "--locked",
                    "--offline",
                    "--no-deps",
                    "--format-version",
                    "1",
                    "--manifest-path",
                ])
                .arg(root.join(workspace).join("Cargo.toml"))
                .output()
                .map_err(|source| StyleError::Io {
                    operation: "Reading Cargo workspace metadata",
                    source,
                })?;

            if !output.status.success() {
                return Err(StyleError::CargoMetadata(
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                ));
            }

            let metadata: Value = serde_json::from_slice(&output.stdout)?;

            let members = metadata["workspace_members"]
                .as_array()
                .ok_or(StyleError::MissingMetadata("workspace members"))?;

            packages.extend(
                metadata["packages"]
                    .as_array()
                    .ok_or(StyleError::MissingMetadata("Cargo packages"))?
                    .iter()
                    .filter(|package| members.contains(&package["id"]))
                    .cloned(),
            );
        }

        Ok(Self { root, packages })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn directories(&self) -> Result<Vec<PathBuf>> {
        self.packages
            .iter()
            .map(|package| {
                let manifest = Path::new(
                    package["manifest_path"]
                        .as_str()
                        .ok_or(StyleError::MissingMetadata("package manifest"))?,
                );

                Ok(manifest
                    .parent()
                    .ok_or(StyleError::MissingMetadata("package directory"))?
                    .strip_prefix(&self.root)?
                    .to_owned())
            })
            .collect()
    }

    pub fn source_targets(&self) -> Result<Vec<SourceTarget>> {
        let mut sources = Vec::new();

        for package in &self.packages {
            let targets = package["targets"]
                .as_array()
                .ok_or(StyleError::MissingMetadata("Cargo targets"))?;

            let library_names: HashSet<_> = targets
                .iter()
                .filter(|target| {
                    target["kind"].as_array().is_some_and(|kinds| {
                        kinds.iter().any(|kind| {
                            matches!(
                                kind.as_str(),
                                Some(
                                    "lib"
                                        | "rlib"
                                        | "dylib"
                                        | "cdylib"
                                        | "staticlib"
                                        | "proc-macro"
                                )
                            )
                        })
                    })
                })
                .filter_map(|target| target["name"].as_str())
                .map(|name| name.replace('-', "_"))
                .collect();

            let dependencies: HashSet<_> = package["dependencies"]
                .as_array()
                .ok_or(StyleError::MissingMetadata("dependency metadata"))?
                .iter()
                .filter_map(|dependency| {
                    dependency["rename"]
                        .as_str()
                        .or_else(|| dependency["name"].as_str())
                })
                .map(|name| name.replace('-', "_"))
                .collect();

            for target in targets {
                let name = target["name"]
                    .as_str()
                    .ok_or(StyleError::MissingMetadata("target name"))?
                    .replace('-', "_");

                let kinds = target["kind"]
                    .as_array()
                    .ok_or(StyleError::MissingMetadata("target kind"))?;

                let external_library = library_names.contains(&name)
                    && kinds.iter().any(|kind| {
                        matches!(kind.as_str(), Some("bin" | "test" | "example" | "bench"))
                    });

                let crate_name = if external_library
                    || dependencies.contains(&name)
                    || kinds.iter().any(|kind| kind == "custom-build")
                {
                    None
                } else {
                    Some(name)
                };

                let path = PathBuf::from(
                    target["src_path"]
                        .as_str()
                        .ok_or(StyleError::MissingMetadata("target source"))?,
                );

                sources.push(SourceTarget { path, crate_name });
            }
        }

        Ok(sources)
    }
}
