use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use suzatary_conversations::secretary::store::RunId;
use suzatary_lines::secretary::run::{Held, Runs};
use suzatary_store::store::StoreError;

/// The runs under one directory, box-local: each run's round holds an `flock` on
/// `<dir>/<run>.lock`, so the kernel lets go of it when the round ends or its process dies, and
/// any process can ask whether it is gone.
#[derive(Debug)]
pub(crate) struct Locks {
    dir: PathBuf,
}

impl Locks {
    #[must_use]
    pub(crate) fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub(crate) fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn path(&self, run: &RunId) -> PathBuf {
        self.dir.join(format!("{run}.lock"))
    }
}

/// A run's lock file, held, and removed before it is let go.
struct Holding {
    path: PathBuf,
    _file: File,
}

impl Drop for Holding {
    fn drop(&mut self) {
        // A file left behind reads as gone too: its lock is free once the file closes.
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Runs for Locks {
    fn begin(&self) -> Result<Held, StoreError> {
        let refused = |failed: &dyn std::fmt::Display| StoreError::unwritable("a run", failed);
        let run = RunId::drawn(&suzatary_io_local::texts::SystemEntropy)
            .map_err(|failed| refused(&failed))?;
        std::fs::create_dir_all(&self.dir).map_err(|failed| refused(&failed))?;
        let path = self.path(&run);
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|failed| refused(&failed))?;
        file.try_lock().map_err(|failed| refused(&failed))?;
        Ok(Held::new(run, Holding { path, _file: file }))
    }

    fn gone(&self, run: &RunId) -> bool {
        let path = self.path(run);
        let Ok(file) = OpenOptions::new().write(true).open(&path) else {
            return true;
        };
        match file.try_lock() {
            Ok(()) => {
                let _ = std::fs::remove_file(&path);
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run is held, against another holder in the same process too, until its hold drops.
    #[test]
    fn a_run_is_gone_once_its_hold_drops() {
        let dir = suzatary_testkit::scratch("runs");
        let runs = Locks::at(dir.path().join("runs"));
        let held = runs.begin().expect("held");
        let run = held.run.clone();
        assert!(!runs.gone(&run));
        assert!(!Locks::at(dir.path().join("runs")).gone(&run));
        drop(held);
        assert!(runs.gone(&run));
        assert!(!dir.path().join("runs").join(format!("{run}.lock")).exists());
    }
}
