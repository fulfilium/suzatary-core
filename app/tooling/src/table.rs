//! The verdict table: `.git/train/gates/<gate>/<half>[-full]/<tree>`, one file of rows per key,
//! only ever appended to.

use crate::gate::Gate;
use crate::oid::Oid;
use crate::{Error, Half, Reading, Row};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Which gates wrote a row: a selective one (`gate`), or a full one (`gate --full`, `nightly`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Selective,
    Full,
}

/// Which rows a reading takes: every gate's, or the full gates' alone (the nightly's question).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seen {
    Any,
    Full,
}

impl Seen {
    #[must_use]
    pub fn scopes(self) -> &'static [Scope] {
        match self {
            Seen::Any => &[Scope::Selective, Scope::Full],
            Seen::Full => &[Scope::Full],
        }
    }
}

/// One gate's rows.
#[derive(Clone, Debug)]
pub struct Table {
    dir: PathBuf,
}

impl Table {
    /// The table of `gate` under `gates`, the repository's `.git/train/gates`.
    #[must_use]
    pub fn at(gates: &Path, gate: &Gate) -> Table {
        Table {
            dir: gates.join(gate.dir()),
        }
    }

    /// The directory of one half's rows from one scope, a file per tree.
    fn key_dir(&self, half: Half, scope: Scope) -> PathBuf {
        let suffix = match scope {
            Scope::Selective => "",
            Scope::Full => "-full",
        };
        self.dir.join(format!("{}{suffix}", half.name()))
    }

    #[must_use]
    pub fn file(&self, half: Half, scope: Scope, tree: &Oid) -> PathBuf {
        self.key_dir(half, scope).join(tree.to_string())
    }

    /// A key's rows, in the order they were written; a key never written has none.
    pub fn rows(&self, half: Half, scope: Scope, tree: &Oid) -> Result<Vec<Row>, Error> {
        let path = self.file(half, scope, tree);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(Error::Io { path, source }),
        };
        Row::parse_all(&text).map_err(|(line, why)| Error::Unreadable {
            path,
            line,
            why: Box::new(why),
        })
    }

    pub fn read(&self, half: Half, seen: Seen, tree: &Oid) -> Result<Reading, Error> {
        seen.scopes()
            .iter()
            .try_fold(Reading::Unjudged, |reading, scope| {
                Ok(reading.join(Reading::of_rows(&self.rows(half, *scope, tree)?)))
            })
    }

    /// Appends `row` to its key's file in one write, as `>>` did.
    pub fn write(&self, half: Half, scope: Scope, tree: &Oid, row: &Row) -> Result<(), Error> {
        let path = self.file(half, scope, tree);
        let failed = |source| Error::Io {
            path: path.clone(),
            source,
        };
        std::fs::create_dir_all(self.key_dir(half, scope)).map_err(failed)?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(format!("{row}\n").as_bytes()))
            .map_err(failed)
    }
}
