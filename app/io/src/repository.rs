//! The data tree as a git checkout: its uncommitted writes as [`Repository`] (its `git diff
//! --stat`), and what git moves when it changes the tree.
//!
//! No method takes a program or an argument from its caller: the agent may ask
//! what is uncommitted, not have this box run something.

use std::path::PathBuf;

use suzatary_services::repository::{Repository, RepositoryError};

use crate::run::{Cmd, run};
use suzatary_os::present::Present as _;

/// The data tree the box's writes land in.
#[derive(Debug, Clone)]
pub struct Checkout {
    data: PathBuf,
}

/// A checkout's git directory, and the one its refs live in: another only for a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDirs {
    pub dir: PathBuf,
    pub common: PathBuf,
}

/// What git moves when it changes the tree: `HEAD`, its branch (or the packed refs) and the
/// index. A pull, merge, checkout, reset, rebase or stash moves one; `git clean`, a copy or an
/// unpack moves none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stamp {
    head: Vec<u8>,
    branch: Vec<u8>,
    index: Option<(u64, std::time::SystemTime)>,
}

impl Checkout {
    #[must_use]
    pub fn new(data: impl Into<PathBuf>) -> Self {
        Self { data: data.into() }
    }

    pub(crate) fn is_repository(&self) -> bool {
        self.data.join(".git").exists()
    }

    /// # Errors
    /// A `.git` file that cannot be read or names no `gitdir`, or a `commondir` that cannot be
    /// read.
    pub fn git_dirs(&self) -> std::io::Result<GitDirs> {
        let dot_git = self.data.join(".git");
        if !dot_git.is_file() {
            return Ok(GitDirs {
                dir: dot_git.clone(),
                common: dot_git,
            });
        }
        let text = std::fs::read_to_string(&dot_git)?;
        let named = suzatary_connect::settings::remote::Pushed::gitdir(&text).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, ".git names no gitdir")
        })?;
        let dir = self.data.join(named);
        let common = match std::fs::read_to_string(dir.join("commondir")).present()? {
            Some(common) => dir.join(common.trim()),
            None => dir.clone(),
        };
        Ok(GitDirs { dir, common })
    }

    /// Read from git's files, with no git process.
    pub(crate) fn stamp(&self) -> std::io::Result<Stamp> {
        let GitDirs { dir, common } = self.git_dirs()?;
        let head = std::fs::read(dir.join("HEAD"))?;
        let branch = match std::str::from_utf8(&head)
            .ok()
            .and_then(|head| head.strip_prefix("ref:"))
        {
            Some(name) => match std::fs::read(common.join(name.trim())).present()? {
                Some(loose) => loose,
                None => std::fs::read(common.join("packed-refs"))
                    .present()?
                    .unwrap_or_default(),
            },
            None => Vec::new(),
        };
        let index = match std::fs::metadata(dir.join("index")).present()? {
            Some(meta) => Some((meta.len(), meta.modified()?)),
            None => None,
        };
        Ok(Stamp {
            head,
            branch,
            index,
        })
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
