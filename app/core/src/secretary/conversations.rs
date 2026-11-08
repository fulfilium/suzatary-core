//! The conversations' stores as one value: the lines, a prodrome at the [`LineEvent`] schema whose
//! words are locked, the keys that open them, box-local and never pushed, where a new key comes
//! from, and every other holder of the words an erasure empties ([`crate::secretary::holder`]).
//!
//! Reading opens each line's words with the keys and takes no lock. A line is written through
//! [`crate::secretary::transcript::push`] alone, which keeps each key before the line that names
//! it is appended, and never takes it back: an orphan key opens nothing.

use crate::capability::{Entropy, ModelSessions, RenderCache};
use crate::secretary::line_event::LineEvent;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::secretary::opened::{Cached, Opened};
use crate::secretary::words::Words;
use crate::store::{Doc, Gets, Prodrome, Reads};

/// The conversations: their lines, their keys, and the source of fresh keys.
#[derive(Clone, Copy)]
pub struct Conversations<'a> {
    /// Every line, its shape in the clear and its words locked.
    pub lines: &'a dyn Prodrome<LineEvent>,
    /// The key of every slot of words not erased, by name.
    pub keys: &'a dyn Doc<KeyName, TextKey>,
    /// Where a new line's keys are drawn from.
    pub entropy: &'a dyn Entropy,
    /// What the box keeps of the HTML the words were rendered to, emptied of a text before its
    /// key is destroyed.
    pub renders: &'a dyn RenderCache,
    /// The model CLI's sessions, deleted before a key of their conversation is destroyed.
    pub sessions: &'a dyn ModelSessions,
    /// The words opened so far, kept in memory, forgotten before their key is destroyed.
    pub opened: &'a Opened,
}

/// The conversations as a reader holds them: no field reaches a write of a line, a key or a
/// render.
#[derive(Clone, Copy)]
pub struct ReadConversations<'a> {
    /// Every line, its shape in the clear and its words locked.
    pub lines: &'a dyn Reads<LineEvent>,
    /// The key of every slot of words not erased, by name.
    pub keys: &'a dyn Gets<KeyName, TextKey>,
    /// The words opened so far, kept in memory.
    pub opened: &'a Opened,
}

impl<'a> Conversations<'a> {
    /// The stores as a reader holds them.
    #[must_use]
    pub fn reads(self) -> ReadConversations<'a> {
        ReadConversations {
            lines: self.lines,
            keys: self.keys,
            opened: self.opened,
        }
    }
}

impl<'a> ReadConversations<'a> {
    /// How a reader opens a slot: from the words kept in memory, else by its key.
    #[must_use]
    pub fn slots(self) -> Cached<'a> {
        Cached::new(self.keys, self.opened)
    }

    /// The key named `name`, or why its words are not here.
    #[must_use]
    pub fn key(&self, name: &KeyName) -> Words<TextKey> {
        self.slots().key(name)
    }
}

// Test support, never production.
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/conversations.rs"]
mod in_memory;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory::{Counting, Forgot, InMemory};
