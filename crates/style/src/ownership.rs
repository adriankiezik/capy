use crate::workspace::Workspace;
use anyhow::{Context as _, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Ownership {
    files: HashMap<PathBuf, HashSet<Option<String>>>,
}

impl Ownership {
    pub fn discover(workspace: &Workspace) -> Result<Self> {
        let mut ownership = Self::default();

        for target in workspace.source_targets()? {
            ownership.file(
                &target.path,
                target.crate_name.as_deref(),
                true,
                &mut HashSet::new(),
            )?;
        }

        Ok(ownership)
    }

    pub fn name(&self, path: &Path) -> Result<Option<&str>> {
        let path = path.canonicalize()?;

        let Some(owners) = self.files.get(&path) else {
            return Ok(None);
        };

        if owners.len() != 1 {
            return Ok(None);
        }

        Ok(owners.iter().next().and_then(|name| name.as_deref()))
    }

    fn file(
        &mut self,
        path: &Path,
        name: Option<&str>,
        root: bool,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        let path = path.canonicalize()?;

        self.files
            .entry(path.clone())
            .or_default()
            .insert(name.map(str::to_owned));

        if !visited.insert(path.clone()) {
            return Ok(());
        }

        let text = fs::read_to_string(&path)?;

        let file = syn::parse_file(&text)
            .with_context(|| format!("Reading modules in {}", path.display()))?;

        let parent = path.parent().context("Missing source parent")?;

        let directory = if root || path.file_name().is_some_and(|file| file == "mod.rs") {
            parent.to_owned()
        } else {
            parent.join(path.file_stem().context("Missing module name")?)
        };

        self.modules(&file.items, &directory, parent, name, visited)
    }

    fn modules(
        &mut self,
        items: &[syn::Item],
        directory: &Path,
        path_directory: &Path,
        name: Option<&str>,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        for item in items {
            let syn::Item::Mod(module) = item else {
                continue;
            };

            let custom = module
                .attrs
                .iter()
                .filter(|attribute| attribute.path().is_ident("path"))
                .find_map(|attribute| {
                    if let syn::Meta::NameValue(value) = &attribute.meta
                        && let syn::Expr::Lit(value) = &value.value
                        && let syn::Lit::Str(path) = &value.lit
                    {
                        Some(path.value())
                    } else {
                        None
                    }
                });

            if let Some((_, items)) = &module.content {
                let directory = match custom {
                    Some(path) => path_directory.join(path),
                    None => directory.join(module.ident.to_string()),
                };

                self.modules(items, &directory, &directory, name, visited)?;
            } else {
                let candidates = match custom {
                    Some(path) => vec![path_directory.join(path)],
                    None => vec![
                        directory.join(format!("{}.rs", module.ident)),
                        directory.join(module.ident.to_string()).join("mod.rs"),
                    ],
                };

                for path in candidates.into_iter().filter(|path| path.is_file()) {
                    self.file(&path, name, false, visited)?;
                }
            }
        }

        Ok(())
    }
}
