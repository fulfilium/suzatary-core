//! A changelog fragment, `changelog.d/<branch>.md`: one change's entries, which the cut folds
//! into a release's section, so no two changes write one hunk.

use std::collections::BTreeMap;

/// Keep a Changelog's kinds of entry, in a section's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A new feature.
    Added,
    /// A change in what exists.
    Changed,
    /// Something to be removed later.
    Deprecated,
    /// Something removed.
    Removed,
    /// A bug fixed.
    Fixed,
    /// A vulnerability closed.
    Security,
}

impl Kind {
    /// Every kind, in a section's order.
    pub const ALL: [Kind; 6] = [
        Kind::Added,
        Kind::Changed,
        Kind::Deprecated,
        Kind::Removed,
        Kind::Fixed,
        Kind::Security,
    ];

    /// The word its `###` heading carries.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Kind::Added => "Added",
            Kind::Changed => "Changed",
            Kind::Deprecated => "Deprecated",
            Kind::Removed => "Removed",
            Kind::Fixed => "Fixed",
            Kind::Security => "Security",
        }
    }
}

/// Entries by kind, each its `- ` line and the lines continuing it. A monoid under
/// [`Fragment::append`], [`Fragment::default`] its unit: a release's section is the fold of its
/// fragments in the order of their names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fragment {
    entries: BTreeMap<Kind, Vec<String>>,
}

/// Why a file is not a fragment, at its line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FragmentError {
    /// Text under no heading.
    #[error("line {0}: text under no `### <kind>` heading")]
    Unheaded(usize),
    /// A `###` heading Keep a Changelog does not name.
    #[error("line {0}: `### {1}` is no Keep a Changelog kind")]
    UnknownKind(usize, String),
    /// A `#` or `##` heading, which only a release writes.
    #[error("line {0}: a fragment holds `### <kind>` headings alone")]
    Heading(usize),
    /// A heading over no entry.
    #[error("line {0}: `### {1}` heads no entry")]
    EmptyKind(usize, String),
    /// No entry at all.
    #[error("it holds no entry")]
    Empty,
}

impl Fragment {
    /// `### <kind>` headings, each over `- ` entries and the lines that continue them.
    ///
    /// # Errors
    /// [`FragmentError`].
    pub fn parse(text: &str) -> Result<Fragment, FragmentError> {
        let mut entries: BTreeMap<Kind, Vec<String>> = BTreeMap::new();
        // The kind being read, its heading's line, and whether an entry has started under it.
        let mut open: Option<(Kind, usize, bool)> = None;
        let close = |open| match open {
            Some((kind, at, false)) => Err(FragmentError::EmptyKind(at, Kind::word(kind).into())),
            _ => Ok(()),
        };
        for (at, line) in (1..).zip(text.lines()) {
            if let Some(word) = line.strip_prefix("### ").map(str::trim) {
                close(open)?;
                let kind = Kind::ALL
                    .into_iter()
                    .find(|k| k.word() == word)
                    .ok_or_else(|| FragmentError::UnknownKind(at, word.into()))?;
                open = Some((kind, at, false));
            } else if line.starts_with('#') {
                return Err(FragmentError::Heading(at));
            } else if let (Some((kind, heading, _)), true) = (open, line.starts_with("- ")) {
                open = Some((kind, heading, true));
                entries.entry(kind).or_default().push(line.into());
            } else if let Some(entry) = open
                .filter(|o| o.2)
                .and_then(|(kind, ..)| entries.get_mut(&kind)?.last_mut())
            {
                entry.push('\n');
                entry.push_str(line);
            } else if !line.trim().is_empty() {
                return Err(FragmentError::Unheaded(at));
            }
        }
        close(open)?;
        for entry in entries.values_mut().flatten() {
            entry.truncate(entry.trim_end().len());
        }
        if entries.is_empty() {
            return Err(FragmentError::Empty);
        }
        Ok(Fragment { entries })
    }

    /// `self`'s entries, then `other`'s, kind by kind.
    #[must_use]
    pub fn append(mut self, other: Fragment) -> Fragment {
        for (kind, more) in other.entries {
            self.entries.entry(kind).or_default().extend(more);
        }
        self
    }

    /// Whether it is the unit, which a release prints as nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Each kind's heading once, in a section's order, over its entries, a blank line between
    /// kinds; [`Fragment::parse`] reads it back as itself.
    #[must_use]
    pub fn print(&self) -> String {
        self.entries
            .iter()
            .map(|(kind, entries)| format!("### {}\n{}\n", kind.word(), entries.join("\n")))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
