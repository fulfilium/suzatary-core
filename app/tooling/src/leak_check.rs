//! What the leak guard said of a public tree.

use crate::Error;
use crate::oid::Oid;
use crate::refusal::Refusal;
use std::fmt;

/// The leak guard's answer over the public tree of one commit, `T` the tree it read.
#[derive(Debug)]
pub enum LeakCheck<T> {
    Clean(T),
    /// It ran whole and found something; its `file:line` hits are on stderr, and nothing reads
    /// them further.
    Hits,
    CouldNotRun(CheckError),
}

/// Why a leak guard said nothing of a tree.
#[derive(Debug)]
pub enum CheckError {
    /// The public tree could not be written, so the guard did not run.
    Unwritten { source: Oid, why: String },
    /// The guard did not run whole (`public-tree.sh --check` exits 2, or a signal stopped it).
    Unrun { source: Oid, status: String },
}

impl<T> LeakCheck<T> {
    /// A `--skip` is taken only on a check that ran whole and found hits: a skipped commit is
    /// never deployed, so a tree that passes is published instead, and one the guard could not
    /// read is no reason to pass it over.
    pub fn skip(self, skip: &Oid) -> Result<(), Error> {
        match self {
            LeakCheck::Hits => Ok(()),
            LeakCheck::Clean(_) => Err(Refusal::SkipPasses(skip.clone()).into()),
            LeakCheck::CouldNotRun(why) => Err(Error::Unrun(format!("{why}. Nothing published."))),
        }
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckError::Unwritten { source, why } => write!(
                f,
                "could not write the public tree of {}, so its leak guard did not run ({why})",
                source.short()
            ),
            CheckError::Unrun { source, status } => write!(
                f,
                "the leak guard could not run over the public tree of {} (it {status}; see above)",
                source.short()
            ),
        }
    }
}
