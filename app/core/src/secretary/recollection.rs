//! The secretary's memory as a reader holds it: the facts and the record in the owner's words
//! alone ([`OwnersRecord`]). The triage agent's record holds one, so a verb on its surface reads
//! the memory and has no store to write it through: every store here is a read half
//! ([`ReadMemories`]), so no write can be expressed through it, even in this module.

use crate::refusal::Refusal;
use crate::secretary::compaction::Reading;
use crate::secretary::memories::ReadMemories;
use crate::secretary::memory::Memory;
use crate::secretary::shown_record::OwnersRecord;

/// The memory, read-only, and how many bytes it may spend in an answer that carries it.
#[derive(Clone, Copy)]
pub struct Recollection<'a> {
    memories: ReadMemories<'a>,
    budget: usize,
}

impl<'a> Recollection<'a> {
    /// `memories` as a reader holds them, carried within `budget` bytes.
    #[must_use]
    pub fn of(memories: ReadMemories<'a>, budget: usize) -> Self {
        Self { memories, budget }
    }

    /// Every standing fact.
    ///
    /// # Errors
    /// [`ReadMemories::read`]'s.
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
