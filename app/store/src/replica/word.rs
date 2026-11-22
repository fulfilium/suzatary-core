//! What the box said at one contact.

use std::collections::BTreeSet;

use prodrome_core::event::Hash;

/// The box's epoch, heads and prodromes, heard at `fetched_at`.
/// A drop or a rollback holds less but has a later epoch all the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// The box's clock over the store, 1 or more.
    pub epoch: u64,
    pub heads: BTreeSet<Hash>,
    /// The prodromes the box held: none for a store holding no node.
    pub geneses: BTreeSet<Hash>,
    /// Milliseconds since the Unix epoch, by the device's clock.
    pub fetched_at: u64,
}

impl Word {
    /// All of the word but the instant it was heard: two words with one key are one word heard
    /// twice.
    #[must_use]
    pub fn key(&self) -> (u64, &BTreeSet<Hash>, &BTreeSet<Hash>) {
        (self.epoch, &self.heads, &self.geneses)
    }
}
