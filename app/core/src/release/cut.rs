//! A release written into the tree: `app/release.toml` and the changelog's section.

use chrono::NaiveDate;

use super::{Completes, Fragment, Released, Version};

/// One release as the tree will hold it: the version and what it completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    /// The version.
    pub version: Version,
    /// What it completed, listed first in its changelog section.
    pub completes: Completes,
    /// A version the tree declares that no tag names (set by hand, never released): its
    /// section becomes this release's, so it neither blocks the count nor stays as a release.
    pub folds: Option<Version>,
}

/// Why a tree cannot take a release.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TreeError {
    /// `CHANGELOG.md` has no `## [Unreleased]` heading to close.
    #[error("CHANGELOG.md has no `## [Unreleased]` heading to close")]
    NoUnreleased,
}

/// The heading a release closes.
const UNRELEASED: &str = "## [Unreleased]";

impl Cut {
    /// The release file this cut writes.
    #[must_use]
    pub fn released(&self) -> Released {
        Released::new(
            self.version,
            self.completes.todos().iter().map(|c| c.todo.clone()),
        )
    }

    /// `changelog` with its Unreleased section closed under this version on `day`, the
    /// completed todos (on `chain`) first, then `fragments`, and a new empty Unreleased above it;
    /// the section of the version it folds, if any, joins it under the same heading.
    ///
    /// # Errors
    /// No Unreleased heading.
    pub fn changelog(
        &self,
        changelog: &str,
        fragments: &Fragment,
        day: NaiveDate,
        chain: &str,
    ) -> Result<String, TreeError> {
        let folded = self.folds.map(|v| format!("## [{v}] - "));
        let all: Vec<&str> = changelog.split_inclusive('\n').collect();
        let heading = |i: usize| {
            folded
                .as_deref()
                .is_some_and(|h| all.get(i).is_some_and(|line| line.starts_with(h)))
        };
        // The folded heading goes with the blank line under it, so its text joins the section
        // above with one blank line between.
        let under = |i: usize| i > 0 && heading(i - 1) && all[i].trim().is_empty();
        let lines: Vec<&str> = (0..all.len())
            .filter(|&i| !heading(i) && !under(i))
            .map(|i| all[i])
            .collect();
        let at = lines
            .iter()
            .position(|line| line.trim_end() == UNRELEASED)
            .ok_or(TreeError::NoUnreleased)?;
        let listed = self.completes.changelog(chain);
        let listed = if listed.is_empty() {
            String::new()
        } else {
            format!("\n{}\n", listed.join("\n"))
        };
        let fragments = if fragments.is_empty() {
            String::new()
        } else {
            format!("\n{}", fragments.print())
        };
        Ok(format!(
            "{before}{UNRELEASED}\n\n## [{version}] - {day}\n{listed}{fragments}{after}",
            before = lines[..at].concat(),
            version = self.version,
            day = day.format("%Y-%m-%d"),
            after = lines[at + 1..].concat(),
        ))
    }
}
