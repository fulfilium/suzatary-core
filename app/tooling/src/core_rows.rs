//! The rows of the public repository's master, read once.

use crate::oid::Oid;
use crate::{Error, git, process};

/// The local name of core's master, as the train last fetched it.
pub const CORE_MASTER: &str = "refs/remotes/core/master";

/// Core's master: unborn before the operator's first push, else its rows.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Core {
    Unborn,
    Master { tip: Oid, rows: Rows },
}

/// Core's master along its first-parent chain, newest first.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Rows(pub Vec<CoreRow>);

/// One commit of core's master and the landed commits its trailers name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CoreRow {
    pub commit: Oid,
    /// Its `Source:` trailers: the landed commit whose public tree it is.
    pub sources: Vec<Oid>,
    /// Its `Skipped:` trailers: landed commits passed over before it, never published.
    pub skipped: Vec<Oid>,
}

impl Core {
    /// Core's master at `tip`, its rows read from `tip`'s first-parent log.
    pub fn read(tip: Oid) -> Result<Core, Error> {
        let log = process::out(&mut git::cmd(&[
            "log",
            "--first-parent",
            "--format=%H%x1f%(trailers:key=Source,valueonly,separator=%x2C)%x1f\
             %(trailers:key=Skipped,valueonly,separator=%x2C)%x1e",
            &tip.to_string(),
        ]))?;
        let oids = |field: &str| -> Result<Vec<Oid>, Error> {
            field
                .split(',')
                .filter(|word| !word.is_empty())
                .map(Oid::parse)
                .collect()
        };
        let rows = log
            .split('\x1e')
            .map(|record| record.trim_start_matches('\n'))
            .filter(|record| !record.is_empty())
            .map(
                |record| match record.split('\x1f').collect::<Vec<_>>()[..] {
                    [commit, sources, skipped] => Ok(CoreRow {
                        commit: Oid::parse(commit)?,
                        sources: oids(sources)?,
                        skipped: oids(skipped)?,
                    }),
                    _ => Err(Error::Malformed(format!(
                        "core's log printed {record:?}, not a commit and its trailers"
                    ))),
                },
            )
            .collect::<Result<_, _>>()?;
        Ok(Core::Master {
            tip,
            rows: Rows(rows),
        })
    }

    #[must_use]
    pub fn rows(&self) -> &Rows {
        static NONE: Rows = Rows(Vec::new());
        match self {
            Core::Unborn => &NONE,
            Core::Master { rows, .. } => rows,
        }
    }

    #[must_use]
    pub fn tip(&self) -> Option<&Oid> {
        match self {
            Core::Unborn => None,
            Core::Master { tip, .. } => Some(tip),
        }
    }
}

impl Rows {
    /// The row whose `Source:` is `rev`.
    #[must_use]
    pub fn published(&self, rev: &Oid) -> Option<&Oid> {
        self.0
            .iter()
            .find(|row| row.sources.contains(rev))
            .map(|row| &row.commit)
    }

    /// The row whose `Skipped:` names `rev`.
    #[must_use]
    pub fn skipped_by(&self, rev: &Oid) -> Option<&Oid> {
        self.0
            .iter()
            .find(|row| row.skipped.contains(rev))
            .map(|row| &row.commit)
    }

    /// The newest landed commit core publishes.
    #[must_use]
    pub fn newest(&self) -> Option<&Oid> {
        self.0.iter().find_map(|row| row.sources.first())
    }
}
