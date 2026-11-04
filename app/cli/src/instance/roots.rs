//! The two roots every path in the instance file resolves under.

use std::path::{Path, PathBuf};

/// A path under a root, or the path itself when it is absolute.
pub(super) fn resolve_under(root: &Path, named: &Path) -> PathBuf {
    if named.is_absolute() {
        named.to_path_buf()
    } else {
        root.join(named)
    }
}

/// The checkout: everything a second deployment shares by cloning the code.
#[derive(Debug, Clone)]
pub struct CodeRoot(pub(super) PathBuf);

impl CodeRoot {
    pub(crate) fn under(&self, named: &Path) -> PathBuf {
        resolve_under(&self.0, named)
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// The data root: everything this deployment keeps private from the code.
#[derive(Debug, Clone)]
pub struct DataRoot(pub(super) PathBuf);

impl DataRoot {
    pub(crate) fn under(&self, named: &Path) -> PathBuf {
        resolve_under(&self.0, named)
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Both roots, so every path in the file must declare which side it is on.
#[derive(Debug, Clone)]
pub struct Roots {
    pub code: CodeRoot,
    pub data: DataRoot,
}
