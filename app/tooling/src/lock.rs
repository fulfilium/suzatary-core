//! One train at a time in a checkout.

use crate::Error;
use crate::instant::Instant;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// The train's lock, `.git/train/lock`, held while this value lives. Its descriptor is the
/// `File`'s own, which std opens close-on-exec and nothing here hands to a command, so no child
/// holds the lock: it is free the moment this process ends, whatever the children left running.
#[derive(Debug)]
pub struct Lock {
    _held: File,
}

/// The train's state directory and the verb that takes its lock, for `lock.holder`.
pub struct Holder<'a> {
    pub state: &'a Path,
    pub verb: &'a str,
}

impl Holder<'_> {
    /// The lock, or the words of the train that holds it.
    pub fn take(&self) -> Result<Lock, Error> {
        let file = self.file()?;
        match file.try_lock() {
            Ok(()) => self.held(file),
            Err(TryLockError::WouldBlock) => Err(Error::Busy(
                std::fs::read_to_string(self.state.join("lock.holder"))
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
            )),
            Err(TryLockError::Error(source)) => Err(self.io(source)),
        }
    }

    /// The lock once the train that holds it lets it go.
    pub fn wait(&self) -> Result<Lock, Error> {
        let file = self.file()?;
        file.lock().map_err(|source| self.io(source))?;
        self.held(file)
    }

    fn file(&self) -> Result<File, Error> {
        std::fs::create_dir_all(self.state)
            .and_then(|()| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.path())
            })
            .map_err(|source| self.io(source))
    }

    fn held(&self, file: File) -> Result<Lock, Error> {
        let holder = self.state.join("lock.holder");
        let line = format!(
            "{}, pid {}, since {}\n",
            self.verb,
            std::process::id(),
            Instant::now()
        );
        std::fs::write(&holder, line).map_err(|source| Error::Io {
            path: holder,
            source,
        })?;
        Ok(Lock { _held: file })
    }

    fn path(&self) -> PathBuf {
        self.state.join("lock")
    }

    fn io(&self, source: std::io::Error) -> Error {
        Error::Io {
            path: self.path(),
            source,
        }
    }
}
