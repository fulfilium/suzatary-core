//! A reading over every conversation, so one unreadable conversation costs only itself and is
//! never a conversation of nothing.

use suzatary_base::conversation_id::ConversationId;

/// A conversation the store holds that does not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    pub id: ConversationId,
    pub why: String,
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "conversation {} does not read: {}", self.id, self.why)
    }
}

/// What a reading over every conversation found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found<T> {
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
