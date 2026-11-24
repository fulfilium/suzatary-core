//! Where a fact stands, as its register orders it: forgotten is above remembered.
//!
//! Two different rememberings of one id are incomparable; the box cannot make one, because ids are
//! counted under the memory's lock.
//! The register is inflationary, so the store refuses a remembering after a forgetting.

use prodrome_data::fold::{Discrete, Inflationary, Order};

use crate::secretary::memory_event::MemoryEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life<'e> {
    Remembered(Discrete<&'e MemoryEvent>),
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
