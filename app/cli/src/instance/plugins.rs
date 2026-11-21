//! `[[plugins]]` as the instance file declares them.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::bail;

use super::DataRoot;
use suzatary_services::plugin::{Manifest, Plugins};

/// `[[plugins]]` with no name twice (a name is a cursor's directory and a credential namespace),
/// and the cache tier's root under the data root.
pub(super) fn plugins_of(
    path: &Path,
    data: &DataRoot,
    cache: Option<&Path>,
    manifests: Vec<Manifest>,
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
