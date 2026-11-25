//! A model's turn as the box writes it, always with its call's carry.

use suzatary_base::capability::Datetime;
use suzatary_conversations::secretary::model::{ToolCall, Turn};
use suzatary_conversations::secretary::store::Entry;
use suzatary_conversations::secretary::taint::Taint;

/// A model's turn to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub at: Datetime,
    pub text: String,
    pub calls: Vec<ToolCall>,
    /// The round's join of every call it built.
    pub carries: Taint,
}

impl Answer {
    /// A turn after calls that carried `carries`, a failed one's included.
    #[must_use]
    pub fn after(at: Datetime, text: String, calls: Vec<ToolCall>, carries: Taint) -> Self {
        Self {
            at,
            text,
            calls,
            carries,
        }
    }

    /// The box's own constant sentence: clean, since no model, verb or store wrote it.
    #[must_use]
    pub fn own(at: Datetime, sentence: &'static str) -> Self {
        Self {
            at,
            text: sentence.to_owned(),
            calls: Vec::new(),
            carries: Taint::Clean,
        }
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
            } => BoxLine::Answer(Answer::after(at, text, calls, carries)),
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
