//! A directory of the tool's own for one run.

use crate::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A directory made empty and new under the system's temporary directory, removed when dropped.
#[derive(Debug)]
pub struct Scratch(PathBuf);

static MADE: AtomicU32 = AtomicU32::new(0);

impl Scratch {
    pub fn new() -> Result<Scratch, Error> {
        let path = std::env::temp_dir().join(format!(
            "train-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.subsec_nanos()),
            MADE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)
            .map(|()| Scratch(path.clone()))
            .map_err(|source| Error::Io { path, source })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
