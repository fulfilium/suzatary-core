//! The plugins this box runs, and the cache tier their stores live in.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::bail;

use super::DataRoot;

/// `[[plugins]]` and the cache tier they write to.
#[derive(Debug, Clone)]
pub struct Plugins {
    /// Every manifest, each name once.
    pub manifests: Vec<suzatary_core::plugin::Manifest>,
    /// The cache tier's root: box-local, never pushed, never staged by `safe-push.sh`.
    pub cache: PathBuf,
}

impl Plugins {
    /// Where `name`'s store lives, at its declared schema, beside its cursor.
    #[must_use]
    pub fn store(&self, name: &suzatary_core::plugin::PluginName) -> PathBuf {
        self.beside(name).join("store")
    }

    /// Every declared plugin's store, which the calendar reads as one where its schema is a
    /// calendar's.
    #[must_use]
    pub fn stores(&self) -> Vec<PathBuf> {
        self.manifests
            .iter()
            .map(|manifest| self.store(manifest.name()))
            .collect()
    }

    /// Where `name`'s cursor and store live.
    #[must_use]
    pub fn beside(&self, name: &suzatary_core::plugin::PluginName) -> PathBuf {
        self.cache.join("plugins").join(name.as_str())
    }

    /// The manifest called `name`.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&suzatary_core::plugin::Manifest> {
        self.manifests
            .iter()
            .find(|manifest| manifest.name().as_str() == name)
    }
}

/// `[[plugins]]` with no name twice (a name is a cursor's directory and a credential namespace),
/// and the cache tier's root under the data root.
pub(super) fn plugins_of(
    path: &Path,
    data: &DataRoot,
    cache: Option<&Path>,
    manifests: Vec<suzatary_core::plugin::Manifest>,
) -> anyhow::Result<Plugins> {
    let mut seen = BTreeSet::new();
    for manifest in &manifests {
        if !seen.insert(manifest.name().as_str()) {
            bail!(
                "{}: the plugin {:?} is declared twice",
                path.display(),
                manifest.name().as_str()
            );
        }
    }
    Ok(Plugins {
        manifests,
        cache: data.under(cache.unwrap_or_else(|| Path::new("cache"))),
    })
}
