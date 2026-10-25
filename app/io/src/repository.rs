//! The data tree's uncommitted writes as [`Repository`]: its `git diff --stat`.
//!
//! No method takes a program or an argument from its caller: the agent may ask
//! what is uncommitted, not have this box run something.

use std::path::PathBuf;

use suzatary_core::capability::{Repository, RepositoryError};

use crate::run::{Cmd, run};

/// The data tree the box's writes land in.
#[derive(Debug)]
pub struct Checkout {
    data: PathBuf,
}

impl Checkout {
    /// The diff of `data`.
    #[must_use]
    pub fn new(data: impl Into<PathBuf>) -> Self {
        Self { data: data.into() }
    }
}

impl Repository for Checkout {
    fn diff_summary(&self) -> Result<String, RepositoryError> {
        let shown = run(&Cmd::new("git").args(["diff", "--stat"]).cwd(&self.data))
            .map_err(|failed| RepositoryError {
                what: "git_diff_summary",
                why: failed.to_string(),
            })?
            .out();
        Ok(if shown.is_empty() {
            "(no uncommitted changes)".to_owned()
        } else {
            shown
        })
    }
}
