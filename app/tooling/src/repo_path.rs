//! A path of a public tree.

use crate::Error;
use std::fmt;

/// A file's path relative to a tree's root, as `git ls-tree` and `find -printf %P` print it: no
/// empty, `.` or `..` part, so it names a place inside the tree and no other.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RepoPath(String);

impl RepoPath {
    pub fn parse(text: &str) -> Result<RepoPath, Error> {
        if text.chars().any(char::is_control)
            || text.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            Err(Error::Malformed(format!(
                "{text:?} is not a path relative to a tree's root"
            )))
        } else {
            Ok(RepoPath(text.to_owned()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Where a move may take it: anywhere among the test trees of its top-level directory, if it
    /// is in one, else within its own directory.
    #[must_use]
    pub(crate) fn area(&self) -> Area<'_> {
        let dir = self.0.rsplit_once('/').map_or("", |(dir, _)| dir);
        if dir
            .split('/')
            .any(|part| matches!(part, "tests" | "test" | "e2e" | "fixtures"))
        {
            Area::Tests(self.0.split_once('/').map_or("", |(top, _)| top))
        } else {
            Area::Dir(dir)
        }
    }
}

/// A part of a tree a move stays within.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Area<'a> {
    /// The test trees (a directory named `tests`, `test`, `e2e` or `fixtures`) of a top-level
    /// directory.
    Tests(&'a str),
    /// One directory outside them, `""` the root.
    Dir(&'a str),
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
