//! Words the owner can erase, sealed per slot, and the facts the secretary keeps in them.

#![forbid(unsafe_code)]

pub mod coding_report;
pub mod secretary {
    //! A line's words, the keys that seal them, the stores that hold copies, and the memory's facts.
    pub mod carried;
    pub mod erasure;
    pub mod fact_life;
    pub mod holder;
    pub mod key_ring;
    pub mod line_words;
    pub mod locked_text;
    pub mod memory;
    pub mod memory_event;
    pub mod provenance;
    pub mod salted_text;
    pub mod scrubbed_under;
    pub mod slots;
    pub mod texts;
    pub mod two_phase;
    pub mod words;
}
