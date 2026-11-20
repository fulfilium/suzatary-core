//! What a public tree holds that no published row holds.

use crate::admitted::Admitted;
use crate::repo_path::RepoPath;

/// One thing a tree holds that core's rows never held, at the path a person would look at.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Fresh {
    /// A file at a path no row has held.
    Path(RepoPath),
    /// A line of the admitted files, a `.gitleaksignore` fingerprint or a change of the secret
    /// rules, at the path it vouches for.
    Admission(RepoPath),
}

impl Fresh {
    #[must_use]
    pub fn path(&self) -> &RepoPath {
        match self {
            Fresh::Path(path) | Fresh::Admission(path) => path,
        }
    }
}

/// Where a public tree is pushed: both publish it, since a deleted branch's commit stays served by
/// its name and a fork shares its objects.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Push {
    /// A row of core's master, which a deployment runs.
    Row,
    /// A gate branch, which the runners gate and the train deletes.
    GateBranch,
}

impl Push {
    /// Whether the push may hold `fresh` with no person's word: a new plain-text file passes both
    /// checks wherever it goes, so only a row needs it admitted; a new admission changes what
    /// the checks pass, so every push does.
    #[must_use]
    pub fn may_hold(self, fresh: &Fresh) -> bool {
        match (self, fresh) {
            (Push::GateBranch, Fresh::Path(_)) => true,
            (Push::Row | Push::GateBranch, Fresh::Admission(_)) | (Push::Row, Fresh::Path(_)) => {
                false
            }
        }
    }

    /// The paths of `fresh` this push may not hold without an admission, sorted, each once.
    #[must_use]
    pub fn held(self, fresh: &[Fresh], admits: &[Admitted]) -> Vec<RepoPath> {
        let mut held: Vec<RepoPath> = fresh
            .iter()
            .filter(|f| !self.may_hold(f) && !admits.iter().any(|a| a.path() == f.path()))
            .map(|f| f.path().clone())
            .collect();
        held.sort();
        held.dedup();
        held
    }
}
