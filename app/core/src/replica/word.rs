//! What the box said at one contact.

use std::collections::BTreeSet;

use prodrome_core::event::Hash;

/// The epoch of what the box served, its heads, and the prodromes it held, heard at
/// `fetched_at`. The epoch is the box's own clock over the store, so a drop or a rollback, which
/// hold less, are later words all the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// The box's clock over the store, 1 or more.
    pub epoch: u64,
    /// The box's heads.
    pub heads: BTreeSet<Hash>,
    /// The prodromes the box held: none for a store holding no node.
    pub geneses: BTreeSet<Hash>,
    /// Milliseconds since the Unix epoch, by the device's clock.
    pub fetched_at: u64,
}

impl Word {
    /// What makes two words one word heard twice: all of it but the instant it was heard.
    #[must_use]
    pub fn key(&self) -> (u64, &BTreeSet<Hash>, &BTreeSet<Hash>) {
        (self.epoch, &self.heads, &self.geneses)
    }
}
