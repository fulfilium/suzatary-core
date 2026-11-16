//! Where a fact stands, as its register orders it: forgotten is above remembered.
//!
//! Unwritten is below every write. A remembering is a whole event, discrete: two different ones
//! about one id are incomparable, a conflict nothing on this box can make, since ids are counted
//! under the memory's lock. Forgotten is the top, above every remembering, so the register is a
//! two-phase set's element (absent, then added, then removed) and declared inflationary: the
//! store refuses a remembering where the reading is already forgotten.

use prodrome_core::fold::{Discrete, Inflationary, Order};

use crate::secretary::memory_event::MemoryEvent;

/// One point of a fact's life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life<'e> {
    /// Remembered, by this event.
    Remembered(Discrete<&'e MemoryEvent>),
    /// Forgotten, whoever forgot it and when.
    Forgotten,
}

impl Order for Life<'_> {
    fn le(&self, other: &Self) -> bool {
        match (self, other) {
            (Life::Remembered(one), Life::Remembered(two)) => one.le(two),
            (_, Life::Forgotten) => true,
            (Life::Forgotten, Life::Remembered(_)) => false,
        }
    }
}

/// The append refuses a remembering above a forgetting, or beside another remembering.
impl Inflationary for Life<'_> {}
