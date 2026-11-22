//! One stored turn, with its instant.

use super::{Entry, stopped_by};
use crate::secretary::model::Turn;
use suzatary_base::capability::Datetime;
use suzatary_inbox::secretary::stamp::Stamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub at: Datetime,
    pub turn: Turn,
    pub by: Option<Stamp>,
}

impl Said {
    /// The turn line `entry` says, as a conversation's fold reads it: a said line's turn, and the
    /// act a stop reads as; `None` for a line that says no turn.
    #[must_use]
    pub fn of(entry: &Entry) -> Option<Said> {
        match entry {
            Entry::Said { at, turn, by } => Some(Said {
                at: *at,
                turn: turn.clone(),
                by: by.clone(),
            }),
            Entry::Stopped { at, by } => Some(Said {
                at: *at,
                turn: Turn::Act {
                    text: stopped_by(by),
                    references: Vec::new(),
                },
                by: Some(by.as_str().into()),
            }),
            Entry::Opened { .. }
            | Entry::SetUp { .. }
            | Entry::Titled { .. }
            | Entry::Proposed { .. }
            | Entry::Reviewed { .. }
            | Entry::ReviewFailed { .. }
            | Entry::Rewound { .. }
            | Entry::MemorySeen { .. }
            | Entry::Session { .. }
            | Entry::Kept { .. }
            | Entry::Unheard { .. }
            | Entry::Scoped { .. }
            | Entry::Coding { .. }
            | Entry::Erased { .. }
            | Entry::Resealed { .. }
            | Entry::Began { .. }
            | Entry::Again { .. } => None,
        }
    }
}
