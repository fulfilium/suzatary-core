//! A path of a public tree.

use crate::Error;
use std::fmt;

/// A file's path relative to a tree's root, as `git ls-tree` and `find -printf %P` print it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RepoPath(String);

impl RepoPath {
    pub fn parse(text: &str) -> Result<RepoPath, Error> {
        if text.is_empty() || text.starts_with('/') || text.chars().any(char::is_control) {
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
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
