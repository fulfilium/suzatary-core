//! What one line does to its conversation's reading, read off its shape: what the rounds' outline
//! folds. Every line's skeleton node carries it, so a reader counts and chooses rounds, and names
//! the lines that hold each round's words, from the skeleton without the line's object.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::secretary::line_shape::LineShape;
use crate::secretary::saying::Saying;
use prodrome_data::event::Hash;
use suzatary_conversations::secretary::store::RunId;
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_inbox::secretary::stamp::Stamp;
use suzatary_memory::secretary::locked_text::KeyName;

/// What a line does to its conversation's reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "step", rename_all = "lowercase")]
#[ts(rename = "LineStep")]
pub enum Step {
    /// Opens the conversation; a later opening line moves nothing. `kind` is what it is for, as
    /// its unsealed shape stores it; none on a line that stores no kind, whose kind its
    /// conversation's id says (`line_shape::unkinded`).
    Opened {
        kind: Option<suzatary_conversations::secretary::store::Kind>,
    },
    /// A turn that opens a round: a person's.
    Asks,
    /// An edit's own question: `Asks`, and takes up the `Again` it rests on.
    Edits,
    /// The box's own note, an act it composed from its own state: it opens a round as an act
    /// does, and a round of nothing but such notes is no round of the owner's.
    Noted,
    /// An act of anyone else's, or the act a stop reads as: it opens a round and says no words.
    Acts,
    /// A model turn with no calls: it goes on with the round before it.
    Answers,
    /// A turn that goes on with the round before it and says no words: a verb's answer, or a
    /// model turn with calls.
    Works,
    /// Sets aside every turn from `to` on.
    Rewound {
        /// The first turn set aside.
        to: usize,
    },
    /// A proposal made.
    Proposed,
    /// A coding session recorded for proposal `proposal`.
    Coding { proposal: usize },
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
        #[ts(as = "Vec<String>")]
        keys: BTreeSet<KeyName>,
    },
    /// Moves no turn and holds no words a round reads.
    Still,
    /// Asks `question` again: sets aside what descends from it beneath this line, and it too for
    /// an edit.
    Again {
        #[ts(as = "String")]
        #[serde(with = "name")]
        question: Hash,
        how: suzatary_conversations::secretary::store::Rewind,
    },
    /// Claims a round: run `run` covers the turns of these lines.
    Began {
        #[ts(as = "Vec<String>")]
        #[serde(with = "names")]
        covers: Vec<Hash>,
        #[ts(as = "String")]
        run: RunId,
    },
}

impl Step {
    /// Whether a line of this step takes up the edit of an `Again` it rests on alone.
    #[must_use]
    pub(crate) fn takes_up(&self) -> bool {
        *self == Step::Edits
    }

    /// Whether a turn of this step says no words a list reads
    /// ([`suzatary_conversations::secretary::search::words`]), whatever its words. A turn of
    /// another step may still say none: its text may be blank.
    #[must_use]
    pub fn says_nothing(&self) -> bool {
        match self {
            Step::Noted | Step::Acts | Step::Works => true,
            Step::Opened { .. }
            | Step::Asks
            | Step::Edits
            | Step::Answers
            | Step::Rewound { .. }
            | Step::Proposed
            | Step::Coding { .. }
            | Step::Titled
            | Step::Resealed { .. }
            | Step::Erased { .. }
            | Step::Still
            | Step::Again { .. }
            | Step::Began { .. } => false,
        }
    }

    /// The step of a line shaped `shape`, as `Stored::fold` folds the line.
    #[must_use]
    pub fn of(shape: &LineShape) -> Self {
        match shape {
            LineShape::Opened { kind, .. } => Step::Opened { kind: *kind },
            LineShape::SetUp { .. } => Step::Opened {
                kind: Some(suzatary_conversations::secretary::store::Kind::Topic),
            },
            LineShape::Said { turn, by, saying } => match (turn, saying) {
                (TurnShape::Act { .. }, _) if *by == Some(Stamp::Box) => Step::Noted,
                (TurnShape::Act { .. }, Saying::Plain) => Step::Acts,
                (TurnShape::Person { .. } | TurnShape::Act { .. }, Saying::Edit) => Step::Edits,
                (TurnShape::Person { .. }, Saying::Plain) => Step::Asks,
                (TurnShape::Model { calls, .. }, _) if calls.is_empty() => Step::Answers,
                (TurnShape::Model { .. } | TurnShape::Tool { .. }, _) => Step::Works,
            },
            LineShape::Stopped { .. } => Step::Acts,
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
            LineShape::Again { question, how, .. } => Step::Again {
                question: question.clone(),
                how: *how,
            },
            LineShape::Began { covers, run } => Step::Began {
                covers: covers.iter().map(|covered| covered.line.clone()).collect(),
                run: run.clone(),
            },
        }
    }
}

/// A line name as its text.
mod name {
    use prodrome_data::event::Hash;
    use serde::{Deserialize as _, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(name: &Hash, to: S) -> Result<S::Ok, S::Error> {
        to.serialize_str(name.as_str())
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<Hash, D::Error> {
        Hash::new(String::deserialize(from)?).map_err(serde::de::Error::custom)
    }
}

/// Line names as their text.
mod names {
    use prodrome_data::event::Hash;
    use serde::{Deserialize as _, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(names: &[Hash], to: S) -> Result<S::Ok, S::Error> {
        to.collect_seq(names.iter().map(Hash::as_str))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<Vec<Hash>, D::Error> {
        Vec::<String>::deserialize(from)?
            .into_iter()
            .map(|name| Hash::new(name).map_err(serde::de::Error::custom))
            .collect()
    }
}
