//! One stored turn, with its instant.

use super::{Entry, stopped_by};
use crate::capability::{Datetime, Turn};
use crate::secretary::stamp::Stamp;

/// One stored turn, with the instant its `said` line carried, so it can be placed on a day or
/// against a proposal's `after_turn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// When it was said.
    pub at: Datetime,
    /// The turn itself.
    pub turn: Turn,
    /// Who stamped it: [`Entry::Said`](super::Entry::Said)'s own `by`.
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
            | Entry::Resealed { .. } => None,
        }
    }
}
