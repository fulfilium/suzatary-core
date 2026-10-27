//! What one line does to its conversation's turns, read off its shape: what the rounds' outline
//! folds (`outline`). Every line's skeleton node carries it, so a reader counts and chooses rounds
//! from the skeleton without the line's object.

use serde::{Deserialize, Serialize};

use crate::secretary::line_shape::LineShape;
use crate::secretary::turn_shape::TurnShape;

/// What a line does to its conversation's turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "step", rename_all = "lowercase")]
#[ts(rename = "LineStep")]
pub enum Step {
    /// Opens the conversation; a later opening line moves nothing.
    Opened,
    /// A turn that opens a round: a person's, an act's, or the act a stop reads as.
    Asks,
    /// A turn that goes on with the round before it: the model's, or a verb's.
    Answers,
    /// Sets aside every turn from `to` on.
    Rewound {
        /// The first turn set aside.
        to: usize,
    },
    /// A proposal made.
    Proposed,
    /// A coding session recorded for proposal `proposal`.
    Coding {
        /// The proposal's index.
        proposal: usize,
    },
    /// Moves no turn.
    Still,
}

impl Step {
    /// The step of a line shaped `shape`, as `Stored::fold` folds the line.
    #[must_use]
    pub fn of(shape: &LineShape) -> Self {
        match shape {
            LineShape::Opened { .. } => Step::Opened,
            LineShape::Said { turn, .. } => match turn {
                TurnShape::Person { .. } | TurnShape::Act { .. } => Step::Asks,
                TurnShape::Model { .. } | TurnShape::Tool { .. } => Step::Answers,
            },
            LineShape::Stopped { .. } => Step::Asks,
            LineShape::Rewound { to, .. } => Step::Rewound { to: *to },
            LineShape::Proposed { .. } => Step::Proposed,
            LineShape::Coding { proposal, .. } => Step::Coding {
                proposal: *proposal,
            },
            LineShape::Titled { .. }
            | LineShape::Reviewed { .. }
            | LineShape::ReviewFailed { .. }
            | LineShape::MemorySeen { .. }
            | LineShape::Session { .. }
            | LineShape::Unheard { .. }
            | LineShape::Kept { .. }
            | LineShape::Scoped { .. }
            | LineShape::Erased { .. }
            | LineShape::Resealed { .. } => Step::Still,
        }
    }
}
