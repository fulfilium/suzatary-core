//! A person's word that they looked at a held path.

use crate::repo_path::RepoPath;
use std::io::IsTerminal;

/// Proof that the tool's standard input is a terminal. It stops cron, CI and an unattended land,
/// but not a shell on the box, which can open a pseudo-terminal (SECURITY.md).
///
/// Law 8: nothing else makes one.
///
/// ```compile_fail,E0603
/// let terminal = suzatary_tooling::admitted::Terminal(());
/// ```
#[derive(Debug)]
pub struct Terminal(());

impl Terminal {
    #[must_use]
    pub fn stdin() -> Option<Terminal> {
        std::io::stdin().is_terminal().then_some(Terminal(()))
    }
}

/// A held path admitted by a person at a terminal: a publication holds it only with one.
///
/// Law 8: no `Admitted` without a [`Terminal`].
///
/// ```compile_fail,E0603
/// use suzatary_tooling::{admitted::Admitted, repo_path::RepoPath};
/// let admitted = Admitted(RepoPath::parse("a.txt").unwrap());
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Admitted(RepoPath);

impl Admitted {
    #[must_use]
    pub fn at_terminal(path: RepoPath, _: &Terminal) -> Admitted {
        Admitted(path)
    }

    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.0
    }
}
