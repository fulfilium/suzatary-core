//! Documents on disk, as an interpreter of [`Doc`]: one file per key, replaced whole. A set
//! writes a sibling, syncs it, renames it over the file and syncs the directory, so a reader sees
//! the old document or the new one, an interrupted write leaves the old one standing, and a kept
//! write survives a crash.

use std::path::PathBuf;
use std::sync::Arc;

use suzatary_core::store::{Doc, Gets, StoreError};

use crate::durable::Mode;
use crate::present::Present as _;

/// Documents, each at the path its key names.
#[derive(Clone)]
pub struct Docs<K: 'static, T: 'static> {
    store: &'static str,
    path: Arc<dyn Fn(&K) -> PathBuf + Send + Sync>,
    codec: &'static DocCodec<K, T>,
    mode: Mode,
}

/// How a document is bytes, both ways; each side sees the key, so a content-addressed store can
/// refuse a value its name does not name.
#[derive(Debug, Clone, Copy)]
pub struct DocCodec<K, T> {
    /// The bytes to keep for this value under this key.
    pub print: fn(&K, &T) -> Result<Vec<u8>, String>,
    /// The value these bytes are under this key; `None` for bytes the key does not name, which
    /// read as absent rather than as something else.
    pub parse: fn(&K, Vec<u8>) -> Result<Option<T>, String>,
}

impl<K, T: serde::Serialize + serde::de::DeserializeOwned> DocCodec<K, T> {
    /// One document as its value's pretty-printed JSON, whatever its key.
    pub const JSON: Self = Self {
        print: |_, value| serde_json::to_vec_pretty(value).map_err(|failed| failed.to_string()),
        parse: |_, bytes| {
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|failed| failed.to_string())
        },
    };
}

/// One document and a lock beside it, which every rewrite holds across its read and its write,
/// so two writers, in one process or two, each keep the other's change.
#[derive(Debug, Clone)]
pub struct Locked<T: 'static> {
    docs: Docs<(), T>,
    lock: PathBuf,
}

impl<T: 'static> Locked<T> {
    /// `docs`, rewritten under `lock`.
    #[must_use]
    pub fn new(docs: Docs<(), T>, lock: PathBuf) -> Self {
        Self { docs, lock }
    }
}

impl<T: Sync + 'static> Locked<T> {
    /// The document, `None` before the first write.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    pub fn get(&self) -> Result<Option<T>, StoreError> {
        self.docs.get(&())
    }

    /// Hold the lock, hand `step` what the document holds, and write what it answers, `None`
    /// writing nothing; answer what the document holds after.
    ///
    /// # Errors
    /// The lock, the read or the write failing.
    pub fn rewrite(
        &self,
        step: impl FnOnce(Option<&T>) -> Option<T>,
    ) -> Result<Option<T>, StoreError> {
        let _held = crate::durable::hold(&self.lock).map_err(|failed| {
            StoreError::unwritable(
                self.docs.store,
                format!("{}: {failed}", self.lock.display()),
            )
        })?;
        let held = self.docs.get(&())?;
        match step(held.as_ref()) {
            Some(next) => {
                self.docs.set(&(), &next)?;
                Ok(Some(next))
            }
            None => Ok(held),
        }
    }
}

impl<K: 'static, T: 'static> std::fmt::Debug for Docs<K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Docs")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

impl<K: 'static, T: 'static> Docs<K, T> {
    /// Documents named `store` in a failure, each at `path(key)`.
    #[must_use]
    pub fn new(
        store: &'static str,
        path: impl Fn(&K) -> PathBuf + Send + Sync + 'static,
        codec: &'static DocCodec<K, T>,
    ) -> Self {
        Self {
            store,
            path: Arc::new(path),
            codec,
            mode: Mode::Readable,
        }
    }

    /// The same documents, each readable by this user alone.
    #[must_use]
    pub fn private(self) -> Self {
        Self {
            mode: Mode::Private,
            ..self
        }
    }

    /// Where one document lives.
    #[must_use]
    pub fn path(&self, key: &K) -> PathBuf {
        (self.path)(key)
    }

    /// The document's size, from the filesystem and before its bytes are read.
    ///
    /// # Errors
    /// [`StoreError::Absent`] and [`StoreError::Unreadable`].
    pub fn size(&self, key: &K) -> Result<u64, StoreError> {
        let path = self.path(key);
        std::fs::metadata(&path)
            .present()
            .map_err(|failed| self.failed_read(&path, &failed))?
            .map(|meta| meta.len())
            .ok_or_else(|| StoreError::Absent {
                what: self.store,
                name: path.display().to_string(),
            })
    }

    fn failed_read(&self, path: &std::path::Path, failed: &std::io::Error) -> StoreError {
        StoreError::unreadable(self.store, format!("{}: {failed}", path.display()))
    }
}

impl<K, T> Gets<K, T> for Docs<K, T>
where
    K: Sync + 'static,
    T: Sync + 'static,
{
    fn get(&self, key: &K) -> Result<Option<T>, StoreError> {
        let path = self.path(key);
        let Some(bytes) = std::fs::read(&path)
            .present()
            .map_err(|failed| self.failed_read(&path, &failed))?
        else {
            return Ok(None);
        };
        (self.codec.parse)(key, bytes)
            .map_err(|why| StoreError::unreadable(self.store, format!("{}: {why}", path.display())))
    }
}

impl<K, T> Doc<K, T> for Docs<K, T>
where
    K: Sync + 'static,
    T: Sync + 'static,
{
    fn set(&self, key: &K, value: &T) -> Result<(), StoreError> {
        let path = self.path(key);
        let unwritable = |why: &dyn std::fmt::Display| {
            StoreError::unwritable(self.store, format!("{}: {why}", path.display()))
        };
        let bytes = (self.codec.print)(key, value).map_err(|why| unwritable(&why))?;
        crate::durable::replace(&path, &bytes, self.mode).map_err(|failed| unwritable(&failed))
    }

    fn remove(&self, key: &K) -> Result<(), StoreError> {
        let path = self.path(key);
        crate::durable::remove(&path).map(|_| ()).map_err(|failed| {
            StoreError::unwritable(self.store, format!("{}: {failed}", path.display()))
        })
    }
}
