//! What can stop the tool.

use std::path::PathBuf;

/// Why `train` could not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Usage(String),
    /// A word or a line not in the table's format.
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
    #[error("git {args}: {why}")]
    Git { args: String, why: String },
}

impl Error {
    /// The status the tool exits with: 64 for a usage error, as every script here, else 3, which
    /// no caller reads as a verdict.
    #[must_use]
    pub fn status(&self) -> u8 {
        match self {
            Error::Usage(_) => 64,
            Error::Malformed(_)
            | Error::Unreadable { .. }
            | Error::Io { .. }
            | Error::Git { .. } => 3,
        }
    }
}
