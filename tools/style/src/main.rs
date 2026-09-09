mod cli;
mod crate_paths;
mod error;
mod ownership;
mod rust;
mod shader;
mod source;
mod staged;
mod structure;
mod workspace;

use crate::error::StyleError;
use anyhow::{Context as _, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("Inspecting {}", path.display()))?;

    if metadata.is_symlink() {
        return Ok(());
    }

    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;

            if !matches!(
                entry.file_name().to_str(),
                Some("target" | ".git" | "vendor")
            ) {
                collect(&entry.path(), files)?;
            }
        }
    } else if matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("rs" | "wgsl")
    ) {
        files.push(path.to_owned());
    }

    Ok(())
}

fn normalize(
    text: &str,
    extension: &str,
    crate_name: Option<&str>,
    preserve_comments: bool,
) -> crate::error::Result<String> {
    match extension {
        "rs" => crate::rust::normalize(text, crate_name, preserve_comments),
        "wgsl" => crate::shader::normalize(text),
        _ => Err(StyleError::SourceType),
    }
}

fn main() -> Result<()> {
    let mut workspace = None;

    let (fix, input) = match crate::cli::Cli::read().action {
        Some(crate::cli::Action::Check(mut check)) => {
            if let Some(changes) = check.staged_changes {
                let discovered = crate::workspace::Workspace::discover()?;

                check.input.paths = crate::staged::affected(&changes, &discovered)?;
                workspace = Some(discovered);

                if check.input.paths.is_empty() {
                    println!("No staged crate changes require style checks");

                    return Ok(());
                }

                println!("Checking {} staged crate(s)", check.input.paths.len());
            }

            (false, check.input)
        }
        Some(crate::cli::Action::Fix(input)) => (true, input),
        None => (false, crate::cli::Input::default()),
    };

    if let Some(language) = input.stdin {
        let extension = match language {
            crate::cli::Language::Rs => "rs",
            crate::cli::Language::Wgsl => "wgsl",
        };

        let mut text = String::new();

        std::io::stdin().read_to_string(&mut text)?;

        let formatted = normalize(&text, extension, input.crate_name.as_deref(), false)?;

        if fix {
            std::io::stdout().write_all(formatted.as_bytes())?;
        } else if text != formatted {
            return Err(StyleError::StdinStyle.into());
        }

        return Ok(());
    }

    let mut files = Vec::new();

    if input.paths.is_empty() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .context("Missing repository directory")?;

        collect(root, &mut files)?;
    } else {
        for path in input.paths {
            collect(&path, &mut files)?;
        }
    }

    files.sort();

    files.dedup();

    if files.is_empty() {
        return Err(StyleError::NoSources.into());
    }

    let workspace = match workspace {
        Some(workspace) => workspace,
        None => crate::workspace::Workspace::discover()?,
    };

    let ownership = crate::ownership::Ownership::discover(&workspace)?;

    let mut changes = Vec::new();

    let mut violations = Vec::new();

    for file in &files {
        let text =
            fs::read_to_string(file).with_context(|| format!("Reading {}", file.display()))?;

        let extension = file
            .extension()
            .and_then(|extension| extension.to_str())
            .context("Missing source extension")?;

        if extension == "rs" {
            let issues = crate::structure::violations(&text, file)
                .with_context(|| format!("Checking {}", file.display()))?;

            if !issues.is_empty() {
                violations.extend(
                    issues
                        .into_iter()
                        .map(|(line, message)| (file, line, message)),
                );

                continue;
            }
        }

        let formatted = normalize(
            &text,
            extension,
            ownership.name(file)?,
            file.file_name().is_some_and(|name| name == "tests.rs"),
        )
        .with_context(|| format!("Checking {}", file.display()))?;

        if formatted != text {
            let line = text
                .lines()
                .zip(formatted.lines())
                .position(|(a, b)| a != b)
                .map_or(text.lines().count() + 1, |line| line + 1);

            changes.push((file, formatted, line));
        }
    }

    if !violations.is_empty() {
        for (file, line, message) in &violations {
            eprintln!("{}:{line}: {message}", file.display());
        }

        return Err(StyleError::Structure(violations.len()).into());
    }

    if !fix && !changes.is_empty() {
        for (file, _, line) in &changes {
            eprintln!(
                "{}:{line}: spacing, comments, self-crate paths, or formatting violates repository style",
                file.display()
            );
        }

        return Err(StyleError::Formatting(changes.len()).into());
    }

    for (file, formatted, _) in &changes {
        fs::write(file, formatted).with_context(|| format!("Writing {}", file.display()))?;
    }

    println!(
        "{} files checked; {} files changed",
        files.len(),
        changes.len()
    );

    Ok(())
}
