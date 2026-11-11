//! The operator's manual and the secretary's brief: the agent's whole curated read surface onto
//! the repository.
//!
//! No method takes a path, and absence is `None` rather than an error because a missing manual is
//! a fact the agent can act on.

/// The repository documents the agent is handed, a closed set because a path would be a file read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manual {
    /// `CLAUDE.md`, the constitution.
    Constitution,
    /// `OPERATOR.md`, the triage contract.
    Operator,
    /// `SECRETARY.md`, the secretary's brief, kept a file so a prompt change shows in a diff.
    Secretary,
}

impl Manual {
    /// The file's name, for the "(… is not present)" the verb answers with.
    #[must_use]
    pub fn file_name(self) -> &'static str {
        match self {
            Manual::Constitution => "CLAUDE.md",
            Manual::Operator => "OPERATOR.md",
            Manual::Secretary => "SECRETARY.md",
        }
    }
}

/// The manuals, as the agent is given them.
pub trait Manuals: Sync {
    /// One of the repository documents the agent is handed, `None` when not present.
    fn document(&self, which: Manual) -> Option<String>;
}
