use super::{AssetError, Result};
use std::{
    collections::HashMap,
    fmt::Debug,
    io,
    path::{Component, Path, PathBuf},
};

pub trait AssetSource: Debug + Send + Sync + 'static {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
}

#[derive(Debug)]
pub struct FileSource {
    root: PathBuf,
}

impl FileSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl AssetSource for FileSource {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(self.root.join(path))
    }
}

#[derive(Debug, Default)]
pub struct EmbeddedSource {
    files: HashMap<PathBuf, &'static [u8]>,
}

impl EmbeddedSource {
    pub fn new<P: AsRef<Path>>(
        files: impl IntoIterator<Item = (P, &'static [u8])>,
    ) -> Result<Self> {
        let files = files
            .into_iter()
            .map(|(path, bytes)| Ok((normalize(path.as_ref())?, bytes)))
            .collect::<Result<_>>()?;

        Ok(Self { files })
    }
}

impl AssetSource for EmbeddedSource {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .map(|bytes| bytes.to_vec())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Embedded asset not found"))
    }
}

pub(super) fn normalize(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            _ => return Err(AssetError::InvalidPath(path.to_owned())),
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(AssetError::InvalidPath(path.to_owned()));
    }

    Ok(normalized)
}
