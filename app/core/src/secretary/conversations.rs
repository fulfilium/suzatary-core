//! The conversations' stores as one value: the lines, a prodrome at the [`LineEvent`] schema whose
//! words are locked, the keys that open them, box-local and never pushed, and where a new key
//! comes from.
//!
//! Reading opens each line's words with the keys and takes no lock. A line is written through
//! [`crate::secretary::transcript::push`] alone, which keeps each key before the line that names
//! it is appended, and never takes it back: an orphan key opens nothing.

use crate::capability::{Entropy, RenderCache};
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

/// Each key drawn one count past the last, so no two are equal: for tests, never a source of
/// keys anyone relies on.
#[cfg(any(test, feature = "nothing"))]
#[derive(Debug, Default)]
pub struct Counting(std::sync::atomic::AtomicU64);

#[cfg(any(test, feature = "nothing"))]
impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        let count = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = count.to_be_bytes()[at % 8] ^ u8::try_from(at % 251).unwrap_or(0);
        }
        Ok(())
    }
}

/// The conversations held in memory, for tests.
#[cfg(any(test, feature = "nothing"))]
#[derive(Default)]
pub struct InMemory {
    /// The lines.
    pub lines: crate::store::InMemory<LineEvent>,
    /// The keys.
    pub keys: crate::store::InMemoryKeys,
    /// Where keys come from.
    pub entropy: Counting,
    /// The blocks a sweep forgot, by source.
    pub forgot: Forgot,
    /// The words opened.
    pub opened: Opened,
}

/// A render cache that keeps nothing and remembers every block it was told to forget, for tests.
#[cfg(any(test, feature = "nothing"))]
#[derive(Debug, Default)]
pub struct Forgot(std::sync::Mutex<Vec<String>>);

#[cfg(any(test, feature = "nothing"))]
impl Forgot {
    /// Every block forgotten, by source, in order.
    #[must_use]
    pub fn sources(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[cfg(any(test, feature = "nothing"))]
impl RenderCache for Forgot {
    fn forget(&self, block: &crate::secretary::answer::Renderable) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(block.as_str().to_owned());
        Ok(())
    }
}

#[cfg(any(test, feature = "nothing"))]
impl InMemory {
    /// The stores as one value.
    #[must_use]
    pub fn conversations(&self) -> Conversations<'_> {
        Conversations {
            lines: &self.lines,
            keys: &self.keys,
            entropy: &self.entropy,
            renders: &self.forgot,
            opened: &self.opened,
        }
    }
}
