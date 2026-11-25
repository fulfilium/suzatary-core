//! A model's turn as the box writes it, always with its call's carry.

use suzatary_base::capability::Datetime;
use suzatary_conversations::secretary::model::{ToolCall, Turn};
use suzatary_conversations::secretary::sentence::{Built, Closing, Outside};
use suzatary_conversations::secretary::store::Entry;
use suzatary_conversations::secretary::taint::Taint;

/// A model's turn to write; only its constructors make one.
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    at: Datetime,
    text: String,
    calls: Vec<ToolCall>,
    /// The round's join of every call it built.
    carries: Taint,
}

impl Answer {
    /// A reply the model made after calls that carried `carries`, a failed one's included.
    #[must_use]
    pub fn replied(at: Datetime, text: String, calls: Vec<ToolCall>, carries: Taint) -> Self {
        Self {
            at,
            text,
            calls,
            carries,
        }
    }

    /// The turn that ends a round short of an answer. The box's own sentence carries what the
    /// calls built carried. A refusal's words come from a store or a verb, not from a call, so
    /// they are always tainted. The model CLI's words and a model-chosen verb name carry the join
    /// once a call was built, and are tainted before any call was.
    #[must_use]
    pub fn closing(at: Datetime, closing: &Closing, built: Built) -> Self {
        let carries = match (closing, built) {
            (Closing::Own(_), built) => built.carries(),
            (Closing::Outside(Outside::Refused(_)), _) | (Closing::Outside(_), Built::Nothing) => {
                Taint::Tainted
            }
            (Closing::Outside(_), Built::Joined(carries)) => carries,
        };
        Self {
            at,
            text: closing.escaped(),
            calls: Vec::new(),
            carries,
        }
    }

    /// What the line stores that the turn carries into a call.
    #[must_use]
    pub fn carries(&self) -> Taint {
        self.carries
    }

    #[must_use]
    pub fn entry(&self) -> Entry {
        Entry::Said {
            at: self.at,
            turn: Turn::Model {
                text: self.text.clone(),
                calls: self.calls.clone(),
            },
            by: None,
        }
    }
}

/// A line the box writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxLine {
    Answer(Answer),
    /// Any line but a model's turn.
    Entry(Entry),
}

impl BoxLine {
    /// `entry`, a model's turn carrying `carries`.
    #[must_use]
    pub fn of(entry: Entry, carries: Taint) -> Self {
        match entry {
            Entry::Said {
                at,
                turn: Turn::Model { text, calls },
                ..
            } => BoxLine::Answer(Answer::replied(at, text, calls, carries)),
            other => BoxLine::Entry(other),
        }
    }

    #[must_use]
    pub fn entry(&self) -> Entry {
        match self {
            BoxLine::Answer(answer) => answer.entry(),
            BoxLine::Entry(entry) => entry.clone(),
        }
    }
}
