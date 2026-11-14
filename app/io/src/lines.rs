//! Logs on disk, as an interpreter of [`Log`]: one file of lines per key in one directory.
//!
//! A line is written whole with one `write_all`; a replacement is [`durable::replace`]d. A key
//! reaches a path only through its [`Layout`], whose stems are slugs.

use std::io::Write as _;
use std::path::PathBuf;

use suzatary_base::refusal::Refusal;
use suzatary_store::store::{Decide, Edit, Log, StoreError};

use crate::durable::{self, Held};
use crate::present::Present as _;

#[derive(Debug, Clone)]
pub struct Lines<K: 'static, E: 'static> {
    dir: PathBuf,
    layout: &'static Layout<K>,
    codec: &'static Codec<E>,
}

/// How keys are files; a file whose stem is no key is not a log.
#[derive(Debug, Clone, Copy)]
pub struct Layout<K> {
    /// What one log is, as a refusal names it.
    pub what: &'static str,
    /// The store as a failure names it.
    pub store: &'static str,
    pub extension: &'static str,
    /// Must be a slug: no `/`, `\` or `.`.
    pub stem: fn(&K) -> String,
    pub key: fn(&str) -> Option<K>,
    /// A file beside the logs whose `flock` [`Log::edit`] holds, if edits are serialised.
    pub lock: Option<&'static str>,
    pub durability: Durability,
}

/// Whether a line is on the disk when its write returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// Handed to the kernel, which writes it out later.
    Written,
    /// Synced before the write returns, for a log a decision outside the box rests on.
    Synced,
}

#[derive(Debug, Clone, Copy)]
pub struct Codec<E> {
    /// What one line is, as a failure names it.
    pub what: &'static str,
    /// Without its newline; must hold none.
    pub print: fn(&E) -> Result<String, String>,
    /// The line a text is; `None` for one the store drops rather than refuses.
    pub parse: fn(&str) -> Result<Option<E>, String>,
}

impl<E: serde::Serialize + serde::de::DeserializeOwned> Codec<E> {
    /// A line as compact JSON, which never holds a newline; a line that will not parse is
    /// refused, never dropped.
    #[must_use]
    pub const fn json(what: &'static str) -> Self {
        Self {
            what,
            print: |line| serde_json::to_string(line).map_err(|failed| failed.to_string()),
            parse: |text| {
                serde_json::from_str(text)
                    .map(Some)
                    .map_err(|failed| failed.to_string())
            },
        }
    }
}

impl<K: 'static, E: 'static> Lines<K, E> {
    #[must_use]
    pub fn new(dir: PathBuf, layout: &'static Layout<K>, codec: &'static Codec<E>) -> Self {
        Self { dir, layout, codec }
    }

    pub fn path(&self, key: &K) -> PathBuf {
        let stem = (self.layout.stem)(key);
        debug_assert!(!stem.contains(['/', '\\', '.']), "a stem is a slug");
        self.dir.join(format!("{stem}.{}", self.layout.extension))
    }

    fn unreadable(&self, why: impl std::fmt::Display) -> StoreError {
        StoreError::unreadable(self.layout.store, why)
    }

    fn unwritable(&self, why: impl std::fmt::Display) -> StoreError {
        StoreError::unwritable(self.layout.store, why)
    }

    fn absent(&self, key: &K) -> StoreError {
        StoreError::Absent {
            what: self.layout.what,
            name: (self.layout.stem)(key),
        }
    }

    fn text(&self, lines: &[E]) -> Result<String, StoreError> {
        let mut text = String::new();
        for line in lines {
            text.push_str(&(self.codec.print)(line).map_err(|why| self.unwritable(why))?);
            text.push('\n');
        }
        Ok(text)
    }

    fn write(&self, mut file: std::fs::File, line: &E) -> Result<(), StoreError> {
        let text = self.text(std::slice::from_ref(line))?;
        debug_assert!(text.matches('\n').count() == 1, "a line holds no newline");
        file.write_all(text.as_bytes())
            .and_then(|()| match self.layout.durability {
                Durability::Written => Ok(()),
                Durability::Synced => file.sync_data(),
            })
            .map_err(|failed| self.unwritable(failed))
    }

    fn replace(&self, key: &K, lines: &[E]) -> Result<(), StoreError> {
        durable::replace(
            &self.path(key),
            self.text(lines)?.as_bytes(),
            durable::Mode::Kept,
        )
        .map_err(|failed| self.unwritable(failed))
    }

    fn held(&self) -> Result<Option<Held>, StoreError> {
        self.layout
            .lock
            .map(|name| {
                durable::hold(&self.dir.join(name)).map_err(|failed| self.unwritable(failed))
            })
            .transpose()
    }
}

impl<K, E> Log<K, E> for Lines<K, E>
where
    K: Ord + Sync + 'static,
    E: Sync + 'static,
{
    fn keys(&self) -> Result<Vec<K>, StoreError> {
        let Some(entries) = std::fs::read_dir(&self.dir)
            .present()
            .map_err(|failed| self.unreadable(failed))?
        else {
            return Ok(Vec::new());
        };
        let mut keys = Vec::new();
        for entry in entries {
            let path = entry.map_err(|failed| self.unreadable(failed))?.path();
            if path.extension().and_then(|e| e.to_str()) != Some(self.layout.extension) {
                continue;
            }
            if let Some(key) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(self.layout.key)
            {
                keys.push(key);
            }
        }
        keys.sort();
        Ok(keys)
    }

    fn lines(&self, key: &K) -> Result<Vec<E>, StoreError> {
        let text = std::fs::read_to_string(self.path(key))
            .present()
            .map_err(|failed| self.unreadable(failed))?
            .ok_or_else(|| self.absent(key))?;
        let mut lines = Vec::new();
        for (at, text) in text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
        {
            let parsed = (self.codec.parse)(text).map_err(|failed| {
                self.unreadable(format!(
                    "{}: line {} is not {}: {failed}",
                    (self.layout.stem)(key),
                    at + 1,
                    self.codec.what
                ))
            })?;
            lines.extend(parsed);
        }
        Ok(lines)
    }

    fn start(&self, key: &K, first: &E) -> Result<(), StoreError> {
        std::fs::create_dir_all(&self.dir).map_err(|failed| self.unwritable(failed))?;
        // Create-new, so a taken name is the filesystem's answer and no exists-check can race it.
        let file = match std::fs::File::create_new(self.path(key)) {
            Ok(file) => file,
            Err(failed) if failed.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(StoreError::Taken {
                    what: self.layout.what,
                    name: (self.layout.stem)(key),
                });
            }
            Err(failed) => return Err(self.unwritable(failed)),
        };
        self.write(file, first)?;
        match self.layout.durability {
            Durability::Written => Ok(()),
            Durability::Synced => std::fs::File::open(&self.dir)
                .and_then(|dir| dir.sync_all())
                .map_err(|failed| self.unwritable(failed)),
        }
    }

    fn push(&self, key: &K, line: &E) -> Result<(), StoreError> {
        let file = std::fs::OpenOptions::new()
            .append(true)
            .open(self.path(key))
            .present()
            .map_err(|failed| self.unwritable(failed))?
            .ok_or_else(|| self.absent(key))?;
        self.write(file, line)
    }

    fn edit(&self, key: &K, decide: &mut Decide<'_, E>) -> Result<Edit<E>, Refusal> {
        std::fs::create_dir_all(&self.dir).map_err(|failed| self.unwritable(failed))?;
        let held = self.held()?;
        let edit = decide(&self.lines_if_any(key)?)?;
        match &edit {
            Edit::Push(line) => {
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.path(key))
                    .map_err(|failed| self.unwritable(failed))?;
                self.write(file, line)?;
            }
            Edit::Replace(lines) => self.replace(key, lines)?,
            Edit::Keep => {}
        }
        drop(held);
        Ok(edit)
    }
}
