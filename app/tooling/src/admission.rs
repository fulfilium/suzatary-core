//! One thing a tree's admission files vouch for.

use crate::Error;
use crate::repo_path::RepoPath;

/// A line of the admitted files, a fingerprint of `.gitleaksignore` or the secret rules' bytes,
/// at the path it vouches for: `<path>\t<word>` as `suzatary leaks --admissions` prints it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Admission {
    pub path: RepoPath,
    /// What it vouches, compared whole: `admitted <digest>`, `finding <fingerprint>` or
    /// `rules <blob>`.
    pub word: String,
}

impl Admission {
    pub fn parse(line: &str) -> Result<Admission, Error> {
        let (path, word) = line.split_once('\t').ok_or_else(|| {
            Error::Malformed(format!("{line:?} is not an admission: <path>\\t<word>"))
        })?;
        Ok(Admission {
            path: RepoPath::parse(path)?,
            word: word.to_owned(),
        })
    }
}
