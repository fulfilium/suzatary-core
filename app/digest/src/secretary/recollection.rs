//! The secretary's memory as a reader holds it: the facts and the record in the owner's words
//! alone ([`OwnersRecord`]); every store here is a read half ([`ReadMemories`]), so no write can
//! be expressed through it.

use crate::secretary::compaction::Reading;
use crate::secretary::memories::ReadMemories;
use crate::secretary::shown_record::OwnersRecord;
use suzatary_base::refusal::Refusal;
use suzatary_lines::timings;
use suzatary_memory::secretary::memory::Memory;

/// The memory, read-only, and how many bytes it may spend in an answer that carries it.
#[derive(Clone, Copy)]
pub struct Recollection<'a> {
    memories: ReadMemories<'a>,
    budget: usize,
}

impl<'a> Recollection<'a> {
    #[must_use]
    pub fn of(memories: ReadMemories<'a>, budget: usize) -> Self {
        Self { memories, budget }
    }

    pub fn facts(&self) -> Result<Memory, Refusal> {
        timings::timed(timings::Phase::Facts, || self.memories.read())
    }

    /// The record of every conversation, in the owner's words alone.
    pub fn record(&self) -> Result<OwnersRecord, Refusal> {
        Reading::of(&self.memories.digests).map(OwnersRecord::of)
    }

    #[must_use]
    pub fn budget(&self) -> usize {
        self.budget
    }
}
