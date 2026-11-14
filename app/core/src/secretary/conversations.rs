//! The conversations' stores as one value: the lines, a prodrome at the [`LineEvent`] schema whose
//! words are locked, the keys that open them, box-local and never pushed, where a new key comes
//! from, and every other holder of the words an erasure empties ([`crate::secretary::holder`]).
//!
//! Reading goes through the lines' one fold ([`Opened`]) and takes no store lock. A line is
//! written through [`crate::secretary::transcript::push`] alone, which keeps each key before the
//! line that names it is appended, and never takes it back: an orphan key opens nothing.

use crate::render_cache::RenderCache;
use crate::secretary::held_lines::{HeldLines, TheBox};
use crate::secretary::line_event::LineEvent;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::secretary::model::Model;
use crate::secretary::model_sessions::ModelSessions;
use crate::secretary::opened::Opened;
use crate::secretary::store::{self, Entry, Stored};
use crate::secretary::transcript;
use crate::secretary::words::{Gone, Words};
use crate::timings::{Phase, timed};
use suzatary_base::capability::{Datetime, Entropy};
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::refusal::{Refusal, Status};
use suzatary_store::store::{Doc, Gets, Prodrome, Reads, StoreError};

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
    /// The lines' fold, holding the words opened so far, forgotten before their key is destroyed.
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
    /// The lines' fold.
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

impl ReadConversations<'_> {
    /// The key named `name`, or why its words are not here: a key the store does not hold, or
    /// cannot read, is missing.
    #[must_use]
    pub fn key(&self, name: &KeyName) -> Words<TextKey> {
        match self.keys.get(name) {
            Ok(Some(key)) => Words::kept(key),
            Ok(None) | Err(_) => Gone::Missing.into(),
        }
    }

    /// `read` of the lines' fold at the store's heads now.
    ///
    /// # Errors
    /// The stores' own.
    pub fn fold<T>(&self, read: impl FnOnce(&mut HeldLines<TheBox>) -> T) -> Result<T, StoreError> {
        let graph = timed(Phase::Lines, || self.lines.graph())?;
        timed(Phase::Opened, || self.opened.read(&graph, self.keys, read))
    }
}

// Test support, never production.
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/conversations.rs"]
mod in_memory;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory::{Counting, Forgot, InMemory};

/// Open a conversation with its first line; an empty `title` leaves it for [`crate::secretary::title::owed`].
///
/// # Errors
/// The store's, and a name already taken.
pub fn start(
    conversations: Conversations<'_>,
    model: &dyn Model,
    id: &ConversationId,
    title: &str,
    kind: store::Kind,
    at: Datetime,
) -> Result<Stored, Refusal> {
    let opened = Entry::Opened {
        at,
        title: crate::secretary::words::Words::kept(title.to_owned()),
        model: model.name(),
        kind,
    };
    transcript::start(conversations, id, &opened)?;
    read(conversations, id)
}

/// One conversation, folded.
///
/// # Errors
/// [`Refusal::not_found`] for an unknown name, [`Refusal::internal`] for an unreadable file.
pub fn read(conversations: Conversations<'_>, id: &ConversationId) -> Result<Stored, Refusal> {
    Ok(transcript::read(conversations, id)?.stored().clone())
}

/// [`read`], opening [`DEFAULT`](suzatary_base::conversation_id::DEFAULT) the first time anything asks
/// for it, and no other id.
///
/// # Errors
/// [`read`]'s own, once the default exists or `id` is any other name, and the store's.
pub fn read_or_open_default(
    conversations: Conversations<'_>,
    model: &dyn Model,
    id: &ConversationId,
    at: Datetime,
) -> Result<Stored, Refusal> {
    if id.as_str() != suzatary_base::conversation_id::DEFAULT.as_str() {
        return read(conversations, id);
    }
    match read(conversations, id) {
        Err(refusal) if refusal.status() == Status::NotFound => {
            match start(
                conversations,
                model,
                id,
                "Secretary",
                store::Kind::Default,
                at,
            ) {
                Err(taken) if taken.status() == Status::Conflict => read(conversations, id),
                opened => opened,
            }
        }
        other => other,
    }
}

/// Every conversation, newest first, each folded to its heading fields.
///
/// # Errors
/// The store's own.
pub fn list(conversations: Conversations<'_>) -> Result<Vec<Stored>, Refusal> {
    transcript::each(conversations)?
        .into_iter()
        .map(|(_, read)| {
            read.map(|read| read.stored().clone())
                .map_err(|failed| Refusal::internal(failed.to_string()))
        })
        .collect()
}

pub fn append(
    conversations: Conversations<'_>,
    id: &ConversationId,
    entry: &Entry,
) -> Result<(), Refusal> {
    transcript::push(conversations, id, entry)
        .map_err(|failed| Refusal::internal(failed.to_string()))
}
