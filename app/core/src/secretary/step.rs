//! What one line does to its conversation's reading, read off its shape: what the rounds' outline
//! folds (`outline`). Every line's skeleton node carries it, so a reader counts and chooses rounds,
//! and names the lines that hold each round's words, from the skeleton without the line's object.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::secretary::line_shape::LineShape;
use crate::secretary::locked_text::KeyName;
use crate::secretary::turn_shape::TurnShape;

/// What a line does to its conversation's reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
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
    /// Names the conversation: the last such line, in causal order, holds the title's words.
    Titled,
    /// Seals slot `of` again, from the seal under `from`, so a reader of that slot may read it from
    /// this line, and `from` is dead once this line's own key opens it or is itself superseded.
    Resealed {
        /// The key of the slot as first written.
        #[ts(as = "String")]
        of: KeyName,
        /// The key of the seal it was sealed again from.
        #[ts(as = "String")]
        from: KeyName,
    },
    /// Erases the words of every slot `keys` names.
    Erased {
        /// The keys.
        #[ts(as = "Vec<String>")]
        keys: BTreeSet<KeyName>,
    },
    /// Moves no turn and holds no words a round reads.
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
            LineShape::Titled { .. } => Step::Titled,
            LineShape::Resealed { of, from } => Step::Resealed {
                of: of.clone(),
                from: from.clone(),
            },
            LineShape::Erased { keys, .. } => Step::Erased { keys: keys.clone() },
            LineShape::Reviewed { .. }
            | LineShape::ReviewFailed { .. }
            | LineShape::MemorySeen { .. }
            | LineShape::Session { .. }
            | LineShape::Unheard { .. }
            | LineShape::Kept { .. }
            | LineShape::Scoped { .. } => Step::Still,
        }
    }
}
