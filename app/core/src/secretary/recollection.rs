//! The secretary's memory as a reader holds it: the facts and the record in the owner's words
//! alone ([`OwnersRecord`]), with no method that writes. The triage agent's record holds one, so
//! a verb on its surface reads the memory and has no store to write it through: the stores are
//! private fields here, and every method below reads.

use crate::refusal::Refusal;
use crate::secretary::compaction::Reading;
use crate::secretary::memories::Memories;
use crate::secretary::memory::Memory;
use crate::secretary::owners_record::OwnersRecord;

/// The memory, read-only, and how many bytes it may spend in an answer that carries it.
#[derive(Clone, Copy)]
pub struct Recollection<'a> {
    memories: Memories<'a>,
    budget: usize,
}

impl<'a> Recollection<'a> {
    /// `memories` as a reader holds them, carried within `budget` bytes.
    #[must_use]
    pub fn of(memories: Memories<'a>, budget: usize) -> Self {
        Self { memories, budget }
    }

    /// Every standing fact.
    ///
    /// # Errors
    /// [`Memories::read`]'s.
    pub fn facts(&self) -> Result<Memory, Refusal> {
        self.memories.read()
    }

    /// The record of every conversation, in the owner's words alone.
    ///
    /// # Errors
    /// [`Reading::of`]'s.
    pub fn record(&self) -> Result<OwnersRecord, Refusal> {
        Reading::of(&self.memories.digests).map(OwnersRecord::of)
    }

    /// The bytes it may spend.
    #[must_use]
    pub fn budget(&self) -> usize {
        self.budget
    }
}
