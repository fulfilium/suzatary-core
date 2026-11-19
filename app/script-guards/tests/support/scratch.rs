//! Where a guard works: the repository it reads and a scratch directory of its own. Both test
//! targets of guards mount this file at their root, each from a crate two levels below it.

use std::path::{Path, PathBuf};

/// The repository root, two levels above `CARGO_MANIFEST_DIR`.
#[must_use]
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("app/<crate>/../.. is the repository")
        .to_path_buf()
}

/// A scratch directory ([`suzatary_testkit::scratch`]) that reads as its path.
pub struct Scratch(suzatary_testkit::TempDir);

impl Scratch {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Scratch(suzatary_testkit::scratch(name))
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.0.path()
    }
}

/// Whether a file named `tool` is in a directory of `PATH`.
#[must_use]
pub fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()))
}
