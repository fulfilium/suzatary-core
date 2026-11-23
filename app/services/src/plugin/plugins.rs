//! The plugins this box runs, and the cache tier their stores live in.

use std::path::PathBuf;

use suzatary_health::health::plugin::PluginName;

use super::Manifest;

/// `[[plugins]]` and the cache tier they write to.
#[derive(Debug, Clone)]
pub struct Plugins {
    /// Every manifest, each name once.
    pub manifests: Vec<Manifest>,
    /// The cache tier's root: box-local, never pushed, never staged by `safe-push.sh`.
    pub cache: PathBuf,
}

impl Plugins {
    /// Where `name`'s store lives, at its declared schema, beside its cursor.
    #[must_use]
    pub fn store(&self, name: &PluginName) -> PathBuf {
        self.beside(name).join("store")
    }

    /// The directory of every plugin's store and cursor, one directory each.
    #[must_use]
    pub fn root(&self) -> PathBuf {
        self.cache.join("plugins")
    }

    /// Every declared plugin's store.
    #[must_use]
    pub fn stores(&self) -> Vec<PathBuf> {
        self.manifests
            .iter()
            .map(|manifest| self.store(manifest.name()))
            .collect()
    }

    fn beside(&self, name: &PluginName) -> PathBuf {
        self.root().join(name.as_str())
    }

    #[must_use]
    pub fn named(&self, name: &str) -> Option<&Manifest> {
        self.manifests
            .iter()
            .find(|manifest| manifest.name().as_str() == name)
    }
}
