//! The data tree's uncommitted writes, as the agent may ask after them.
//!
//! One fixed command with nothing interpolated, so this is not a shell.

/// The diff could not be taken.
#[derive(Debug, thiserror::Error)]
#[error("{what} could not be run: {why}")]
pub struct RepositoryError {
    /// Which read.
    pub what: &'static str,
    /// What went wrong.
    pub why: String,
}

/// The data tree this application writes into.
pub trait Repository: Sync {
    /// A `--stat` summary of uncommitted changes.
    fn diff_summary(&self) -> Result<String, RepositoryError>;
}
