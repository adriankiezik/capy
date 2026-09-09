use super::{AssetSource, FileSource};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug)]
pub struct AssetSettings {
    pub(super) source: Arc<dyn AssetSource>,
    pub(super) workers: usize,
}

impl Default for AssetSettings {
    fn default() -> Self {
        Self {
            source: Arc::new(FileSource::new("assets")),
            workers: 2,
        }
    }
}

impl AssetSettings {
    #[must_use]
    pub fn with_workers(mut self, workers: usize) -> Self {
        self.workers = workers;

        self
    }

    #[must_use]
    pub fn with_root(self, root: impl Into<PathBuf>) -> Self {
        self.with_source(FileSource::new(root))
    }

    #[must_use]
    pub fn with_source(mut self, source: impl AssetSource) -> Self {
        self.source = Arc::new(source);

        self
    }
}
