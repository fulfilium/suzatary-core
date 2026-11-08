//! The names of a repository, a path in it and a revision.
//!
//! Every name comes from a model that reads attacker-controlled mail and ends up as a `git`
//! argument, so each constructor refuses anything `git` could read as an option or as a path
//! outside the repository.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why a name was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RepoNameError {
    /// Not `owner/name`, or a part holds a character GitHub does not allow.
    #[error("a repository is owner/name, letters, digits, '-', '_' and '.', got {0:?}")]
    Repository(String),
    /// Absolute, empty in the middle, `..`, or too long.
    #[error("a path is relative, inside the repository and without '..', got {0:?}")]
    Path(String),
    /// Not a full commit id.
    #[error("a commit is forty lowercase hex digits, got {0:?}")]
    Commit(String),
    /// Not a branch, tag or commit name, or one `git` could read as an expression.
    #[error(
        "a revision is a branch, a tag or a commit: letters, digits, '-', '_', '.' and '/', \
         got {0:?}"
    )]
    Rev(String),
    /// Empty, too large, not a unified diff, or a binary change.
    #[error("{0}")]
    Patch(String),
    /// The declared list names repositories of more than one account.
    #[error(
        "every declared repository is in the owner's own account, but these name {}",
        .0.join(", ")
    )]
    Accounts(Vec<String>),
    /// The declared list names one repository twice.
    #[error("{0} is declared twice")]
    Twice(String),
}

fn segment_ok(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 100
        && !part.starts_with('.')
        && !part.starts_with('-')
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `owner/name`, as GitHub spells one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoName {
    owner: String,
    name: String,
}

impl RepoName {
    /// The owner, the account the token belongs to.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The repository's own name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl TryFrom<String> for RepoName {
    type Error = RepoNameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<RepoName> for String {
    fn from(repo: RepoName) -> Self {
        repo.to_string()
    }
}

impl std::str::FromStr for RepoName {
    type Err = RepoNameError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (owner, name) = text
            .split_once('/')
            .ok_or_else(|| RepoNameError::Repository(text.to_owned()))?;
        if !segment_ok(owner) || !segment_ok(name) || name.contains('/') {
            return Err(RepoNameError::Repository(text.to_owned()));
        }
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
}

impl fmt::Display for RepoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// A path inside a repository, relative; `""` is the root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoPath(String);

impl RepoPath {
    /// The path as `git` reads it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RepoPath {
    type Error = RepoNameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        let trimmed = text.trim_end_matches('/');
        let refused = trimmed.len() > 400
            || trimmed.starts_with('/')
            || trimmed.starts_with('-')
            || trimmed.contains('\0')
            || trimmed.contains(':')
            || (!trimmed.is_empty()
                && trimmed
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == ".."));
        if refused {
            return Err(RepoNameError::Path(text));
        }
        Ok(Self(trimmed.to_owned()))
    }
}

impl From<RepoPath> for String {
    fn from(path: RepoPath) -> Self {
        path.0
    }
}

/// A full commit id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommitId(String);

impl CommitId {
    /// Forty hex digits.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first twelve, for a card.
    #[must_use]
    pub fn short(&self) -> &str {
        &self.0[..12]
    }
}

impl TryFrom<String> for CommitId {
    type Error = RepoNameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.len() == 40 && text.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
            Ok(Self(text))
        } else {
            Err(RepoNameError::Commit(text))
        }
    }
}

impl From<CommitId> for String {
    fn from(commit: CommitId) -> Self {
        commit.0
    }
}

/// A revision as the secretary names one: a branch, a tag or a commit id. Nothing `git` reads
/// as an expression (`^`, `~`, `:`, `@{`, `..`) or as an option can be one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rev(String);

impl Rev {
    /// The name as `git rev-parse` reads it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Rev {
    type Error = RepoNameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        let ok = !text.is_empty()
            && text.len() <= 200
            && text.split('/').all(|part| {
                !part.is_empty()
                    && !part.starts_with(['.', '-'])
                    && !part.contains("..")
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            });
        if ok {
            Ok(Self(text))
        } else {
            Err(RepoNameError::Rev(text))
        }
    }
}

impl From<Rev> for String {
    fn from(rev: Rev) -> Self {
        rev.0
    }
}

impl From<CommitId> for Rev {
    fn from(commit: CommitId) -> Self {
        Self(commit.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_is_owner_and_name_and_nothing_git_reads_as_an_option() {
        let repo: RepoName = "bmabsout/resume_typst".parse().expect("parses");
        assert_eq!(repo.owner(), "bmabsout");
        assert_eq!(repo.to_string(), "bmabsout/resume_typst");
        for bad in [
            "bmabsout", "/x", "a/b/c", "-x/y", "a/..", "a/.git", "a b/c", "a/",
        ] {
            assert!(bad.parse::<RepoName>().is_err(), "{bad}");
        }
    }

    #[test]
    fn a_path_stays_inside_the_repository() {
        assert_eq!(
            RepoPath::try_from("cv.typ".to_owned())
                .expect("ok")
                .as_str(),
            "cv.typ"
        );
        assert_eq!(
            RepoPath::try_from(String::new()).expect("root").as_str(),
            ""
        );
        assert_eq!(
            RepoPath::try_from("docs/".to_owned())
                .expect("dir")
                .as_str(),
            "docs"
        );
        for bad in [
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "-n",
            "HEAD:x",
            "./a",
        ] {
            assert!(RepoPath::try_from(bad.to_owned()).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_revision_is_a_name_never_an_expression() {
        for good in ["main", "v1.2", "feature/cv-2026", &"a".repeat(40)] {
            assert!(Rev::try_from(good.to_owned()).is_ok(), "{good}");
        }
        for bad in [
            "", "-n", "main^", "main~1", "HEAD@{1}", "a..b", "a:b", "/main", "main/", "a//b",
            ".hidden", "a b",
        ] {
            assert!(Rev::try_from(bad.to_owned()).is_err(), "{bad}");
        }
        let commit = CommitId::try_from("b".repeat(40)).expect("a commit");
        assert_eq!(Rev::from(commit.clone()).as_str(), commit.as_str());
    }

    #[test]
    fn a_commit_is_a_full_id() {
        assert!(CommitId::try_from("a".repeat(40)).is_ok());
        assert!(CommitId::try_from("HEAD".to_owned()).is_err());
        assert!(CommitId::try_from("A".repeat(40)).is_err());
    }
}
