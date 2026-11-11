//! Reading the owner's repositories; publishing is a tap's outward act and lives elsewhere.

pub use crate::repo_ids::{CommitId, RepoName, RepoPath, Rev};
pub(crate) use crate::repos::RepoRow;
pub use crate::repos::{Patch, Visibility};

/// What one email may carry, in bytes.
pub const MAX_FILE_BYTES: u64 = crate::secretary::attached::MAX_TOTAL;

/// The most of one file [`readable`] shows, in bytes.
pub const MAX_TEXT_BYTES: usize = 100 * 1024;

/// Why a read produced nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RepoFailure {
    /// Undeclared, unreachable by the token, or nonexistent.
    #[error("no repository {0} among those this deployment declares and its GitHub token reaches")]
    Unknown(String),
    /// No such path or commit.
    #[error("not found: {0}")]
    Missing(String),
    /// Over [`MAX_FILE_BYTES`].
    #[error("{path} is {size} bytes; at most {MAX_FILE_BYTES} of one file is read")]
    TooLarge {
        /// The file.
        path: String,
        /// Its size.
        size: u64,
    },
    /// The patch does not apply.
    #[error("the patch does not apply: {0}")]
    Conflict(String),
    /// Public, and its entry does not say `public = true`.
    #[error(
        "{0} is public, and its [[secretary.repos]] entry does not say public = true, so no \
         patch goes there"
    )]
    Public(String),
    /// GitHub, the network or `git` itself.
    #[error("unavailable: {0}")]
    Unavailable(String),
}

/// The repositories this deployment declares, as text.
pub trait Repositories: Sync {
    /// Every repository the token can reach.
    ///
    /// # Errors
    /// GitHub cannot be asked.
    fn repos(&self) -> Result<Vec<RepoRow>, RepoFailure>;

    /// Freshly fetched; `None` is the default branch's tip. A commit resolves to itself, so a
    /// resolved revision means the same thing at any time.
    ///
    /// # Errors
    /// [`RepoFailure`].
    fn resolve(&self, repo: &RepoName, at: Option<&Rev>) -> Result<CommitId, RepoFailure>;

    /// Directories end in `/`.
    ///
    /// # Errors
    /// [`RepoFailure`].
    fn tree(
        &self,
        repo: &RepoName,
        at: &CommitId,
        dir: &RepoPath,
    ) -> Result<Vec<String>, RepoFailure>;

    /// One file's bytes, whole, text or not.
    ///
    /// # Errors
    /// [`RepoFailure`].
    fn blob(&self, repo: &RepoName, at: &CommitId, path: &RepoPath)
    -> Result<Vec<u8>, RepoFailure>;

    /// As `path:line:text`, capped.
    ///
    /// # Errors
    /// [`RepoFailure`].
    fn grep(
        &self,
        repo: &RepoName,
        at: &CommitId,
        needle: &str,
    ) -> Result<Vec<String>, RepoFailure>;

    /// Touches nothing.
    ///
    /// # Errors
    /// [`RepoFailure::Conflict`] with `git apply`'s own words.
    fn applies(&self, repo: &RepoName, base: &CommitId, patch: &Patch) -> Result<(), RepoFailure>;

    /// The freshly fetched default branch, when its tip already holds the change: the patch
    /// undoes cleanly there, or a three-way apply changes nothing.
    ///
    /// # Errors
    /// [`RepoFailure`].
    fn merged(&self, repo: &RepoName, patch: &Patch) -> Result<Option<InBranch>, RepoFailure>;

    /// As the instance file declares it ([`crate::repos::Declared::pushable`]).
    ///
    /// # Errors
    /// [`RepoFailure::Unknown`] or [`RepoFailure::Public`].
    fn pushable(&self, repo: &RepoName, visibility: Visibility) -> Result<(), RepoFailure>;
}

/// A branch that already holds a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InBranch {
    /// The branch's name.
    pub branch: String,
    /// The tip that holds it.
    pub tip: CommitId,
}

/// Cut at [`MAX_TEXT_BYTES`], saying so.
///
/// # Errors
/// A binary file, which is attached, never read.
pub fn readable(path: &RepoPath, bytes: &[u8]) -> Result<String, RepoFailure> {
    if bytes.contains(&0) {
        return Err(RepoFailure::Missing(format!(
            "{} is binary; only text files can be read, and propose_email attaches one",
            path.as_str()
        )));
    }
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= MAX_TEXT_BYTES {
        return Ok(text.into_owned());
    }
    let mut cut = MAX_TEXT_BYTES;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    Ok(format!(
        "{}\n[cut at {MAX_TEXT_BYTES} of {} bytes]",
        &text[..cut],
        text.len()
    ))
}
