//! What can stop the tool.

use crate::refusal::Refusal;
use std::path::PathBuf;

/// Why `train` could not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Refused(#[from] Refusal),
    /// The train's own words, for a gate it cannot run or a verdict that lands nothing.
    #[error("{0}")]
    Said(String),
    #[error("another scripts/train.sh is running in this checkout ({0}); wait for it to finish.")]
    Busy(String),
    /// A signal stopped the run before it wrote anything; the status is a shell's, 128 plus it.
    #[error("stopped by signal {0}")]
    Stopped(i32),
    /// A step the tool needs could not run, so it decided nothing: the leak guard, the matcher,
    /// a fetch or a push.
    #[error("{0}")]
    Unrun(String),
    /// A word or a line not in the format it should be in.
    #[error("{0}")]
    Malformed(String),
    #[error("{}:{line}: {why}", path.display())]
    Unreadable {
        path: PathBuf,
        line: usize,
        why: Box<Error>,
    },
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{command}: {why}")]
    Run { command: String, why: String },
}

impl Error {
    /// The status the tool exits with: 64 for a usage error, as every script here, 1 for a
    /// refusal, else 3, which no caller reads as a verdict.
    #[must_use]
    pub fn status(&self) -> u8 {
        match self {
            Error::Usage(_) => 64,
            Error::Refused(_) | Error::Said(_) | Error::Busy(_) => 1,
            Error::Stopped(signal) => u8::try_from(128 + signal).unwrap_or(1),
            Error::Unrun(_)
            | Error::Malformed(_)
            | Error::Unreadable { .. }
            | Error::Io { .. }
            | Error::Run { .. } => 3,
        }
    }
}
