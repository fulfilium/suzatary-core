//! A reading over every conversation: what the ones that read hold, and each one that does not,
//! named with why, so one unreadable conversation costs only itself and is never a conversation
//! of nothing.

use crate::conversation_id::ConversationId;

/// A conversation the store holds that does not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The conversation.
    pub id: ConversationId,
    /// The reader's words for why.
    pub why: String,
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "conversation {} does not read: {}", self.id, self.why)
    }
}

/// What a reading over every conversation found: its items, from the conversations that read,
/// and each conversation that did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found<T> {
    /// What the readable conversations hold.
    pub items: Vec<T>,
    /// Every conversation that does not read, in the store's order.
    pub unreadable: Vec<Unreadable>,
}

impl<T> Found<T> {
    /// The same reading, each readable conversation's items mapped by `f`.
    #[must_use]
    pub fn flat_map<U, I: IntoIterator<Item = U>>(self, f: impl FnMut(T) -> I) -> Found<U> {
        Found {
            items: self.items.into_iter().flat_map(f).collect(),
            unreadable: self.unreadable,
        }
    }
}
