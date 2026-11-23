//! Documents on disk, as an interpreter of [`Doc`]: one file per key, replaced whole and
//! atomically (sibling, sync, rename, sync the directory).

use std::path::PathBuf;
use std::sync::Arc;

use suzatary_store::store::{Doc, Gets, StoreError};

use suzatary_os::durable::Mode;
use suzatary_os::present::Present as _;

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
    pub print: fn(&K, &T) -> Result<Vec<u8>, String>,
    /// `None` for bytes the key does not name, which read as absent.
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
    #[must_use]
    pub fn new(docs: Docs<(), T>, lock: PathBuf) -> Self {
        Self { docs, lock }
    }
}

impl<T: Sync + 'static> Locked<T> {
    /// The document, `None` before the first write.
    pub fn get(&self) -> Result<Option<T>, StoreError> {
        self.docs.get(&())
    }

    /// Hold the lock, hand `step` what the document holds, and write what it answers, `None`
    /// writing nothing; answer what the document holds after.
    pub fn rewrite(
        &self,
        step: impl FnOnce(Option<&T>) -> Option<T>,
    ) -> Result<Option<T>, StoreError> {
        let _held = suzatary_os::durable::hold(&self.lock).map_err(|failed| {
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

/// The directory a digest's document is in: its first two characters.
pub(crate) fn shard(digest: &str) -> String {
    digest.chars().take(2).collect()
}

impl<T: 'static> Docs<String, T> {
    /// Private documents under `dir`, each named by a hex digest, in the directory [`shard`] names.
    #[must_use]
    pub(crate) fn sharded(
        store: &'static str,
        dir: PathBuf,
        codec: &'static DocCodec<String, T>,
    ) -> Self {
        Docs::new(
            store,
            move |digest: &String| dir.join(shard(digest)).join(digest),
            codec,
        )
        .private()
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

    #[must_use]
    pub fn path(&self, key: &K) -> PathBuf {
        (self.path)(key)
    }

    /// The document's size, from the filesystem and before its bytes are read.
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

    fn unwritable(&self, paths: &[&std::path::Path], why: &dyn std::fmt::Display) -> StoreError {
        let named = match paths {
            [one] => one.display().to_string(),
            [first, rest @ ..] => format!("{} and {} more", first.display(), rest.len()),
            [] => String::new(),
        };
        StoreError::unwritable(self.store, format!("{named}: {why}"))
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
        self.set_all(&[(key, value)])
    }

    fn remove(&self, key: &K) -> Result<(), StoreError> {
        self.remove_all(&[key])
    }

    /// One [`suzatary_os::durable::replace_all`]: the batch's bytes synced together, then each
    /// directory once.
    fn set_all(&self, values: &[(&K, &T)]) -> Result<(), StoreError> {
        let paths: Vec<std::path::PathBuf> = values.iter().map(|(key, _)| self.path(key)).collect();
        let named: Vec<&std::path::Path> = paths.iter().map(std::path::PathBuf::as_path).collect();
        let bytes = values
            .iter()
            .zip(&named)
            .map(|((key, value), path)| {
                (self.codec.print)(key, value).map_err(|why| self.unwritable(&[path], &why))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let files: Vec<(&std::path::Path, &[u8])> = named
            .iter()
            .copied()
            .zip(bytes.iter().map(Vec::as_slice))
            .collect();
        suzatary_os::durable::replace_all(&files, self.mode)
            .map_err(|failed| self.unwritable(&[&failed.path], &failed.error))
    }

    /// One [`suzatary_os::durable::remove_all`]: each directory synced once.
    fn remove_all(&self, keys: &[&K]) -> Result<(), StoreError> {
        let paths: Vec<std::path::PathBuf> = keys.iter().map(|key| self.path(key)).collect();
        let named: Vec<&std::path::Path> = paths.iter().map(std::path::PathBuf::as_path).collect();
        suzatary_os::durable::remove_all(&named)
            .map(|_| ())
            .map_err(|failed| self.unwritable(&named, &failed))
    }
}
