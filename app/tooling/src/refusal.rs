//! Why `train publish` did nothing.

use crate::oid::Oid;
use crate::repo_path::RepoPath;
use std::fmt;

/// A publish refused before it pushed anything; each says why and what to do.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Refusal {
    EmptyRev,
    NoCore,
    RevUnknown(String),
    SkipUnknown(String),
    AdmitAway(RepoPath),
    NotLanded(Oid),
    Skipped { rev: Oid, by: Oid },
    Climbs { newest: Oid, rev: Oid },
    SkipNotARow(Oid),
    SkipIsLast(Oid),
    SkipPasses(Oid),
    AdmitNotHeld(RepoPath),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::EmptyRev => write!(f, "publish was given an empty <rev>."),
            Refusal::NoCore => write!(
                f,
                "this checkout has no remote `core`, the repository a deployment runs: git remote \
                 add core <its url>."
            ),
            Refusal::RevUnknown(word) => write!(f, "{word} names no commit."),
            Refusal::SkipUnknown(word) => write!(f, "--skip {word:?} names no commit."),
            Refusal::AdmitAway(path) => write!(
                f,
                "--admit {path} is a person's word that they looked at it, so it is taken at a \
                 terminal and never from a script."
            ),
            Refusal::NotLanded(rev) => write!(
                f,
                "{} is not on master's first-parent chain: only a landed commit is published.",
                rev.short()
            ),
            Refusal::Skipped { rev, by } => write!(
                f,
                "{} was skipped by publish, never published ({} of core's master names it in a \
                 Skipped: trailer): its tree did not pass the leak guard.",
                rev.short(),
                by.short()
            ),
            Refusal::Climbs { newest, rev } => write!(
                f,
                "core publishes {}, and {} does not descend from it: the table only climbs.",
                newest.short(),
                rev.short()
            ),
            Refusal::SkipNotARow(skip) => write!(
                f,
                "--skip {} is not a commit this publish would make a row of.",
                skip.short()
            ),
            Refusal::SkipIsLast(skip) => write!(
                f,
                "--skip {} is the last commit to publish, so no row after it would record the \
                 skip; publish again once another commit lands.",
                skip.short()
            ),
            Refusal::SkipPasses(skip) => write!(
                f,
                "--skip {}: {skip} passes its leak guard; publish it instead.",
                skip.short()
            ),
            Refusal::AdmitNotHeld(path) => write!(
                f,
                "--admit {path} names no path this publish would hold: admit only what a held \
                 row names."
            ),
        }?;
        f.write_str(" Nothing published.")
    }
}
impl std::error::Error for Refusal {}
