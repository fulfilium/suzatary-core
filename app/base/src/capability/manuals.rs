/// The repository documents the agent is handed, a closed set because a path would be a file read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manual {
    Constitution,
    Operator,
    /// A file so that a prompt change shows in a diff.
    Secretary,
}

impl Manual {
    #[must_use]
    pub fn file_name(self) -> &'static str {
        match self {
            Manual::Constitution => "CLAUDE.md",
            Manual::Operator => "OPERATOR.md",
            Manual::Secretary => "SECRETARY.md",
        }
    }
}

pub trait Manuals: Sync {
    /// `None` when the file is absent, because the agent can act on a missing manual.
    fn document(&self, which: Manual) -> Option<String>;
}
