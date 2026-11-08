//! A coding session's report, as its branch carries it.

use serde::{Deserialize, Serialize};

use crate::repo_ids::RepoPath;

/// The most of a report a card carries, in bytes; the rest is cut and says so.
pub(crate) const MAX_REPORT_BYTES: usize = 64 * 1024;

/// How many lines one file of a branch changed, as `git diff --numstat`
/// says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Lines {
    /// A text file.
    Text {
        /// Lines added.
        added: u32,
        /// Lines removed.
        removed: u32,
    },
    /// A binary file, which has no lines.
    Binary,
}

/// One file of a branch's diff against the default branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStat {
    /// The file.
    pub path: RepoPath,
    /// What changed in it.
    pub lines: Lines,
}

/// A session's report: the file, its text and the branch's diff stat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    /// `docs/reports/<slug>.md`.
    pub path: RepoPath,
    /// What the session wrote: commits, gates, open questions.
    pub text: String,
    /// Every file the branch changes.
    pub stat: Vec<FileStat>,
}

impl Report {
    /// A report, its text cut to `MAX_REPORT_BYTES` at a character
    /// boundary with a line saying where.
    #[must_use]
    pub fn new(path: RepoPath, text: &str, stat: Vec<FileStat>) -> Self {
        let text = if text.len() <= MAX_REPORT_BYTES {
            text.to_owned()
        } else {
            let end = (0..=MAX_REPORT_BYTES)
                .rev()
                .find(|at| text.is_char_boundary(*at))
                .unwrap_or_default();
            format!(
                "{}\n\n[cut: the report is {} bytes; the rest is on the branch]",
                &text[..end],
                text.len()
            )
        };
        Self { path, text, stat }
    }
}
