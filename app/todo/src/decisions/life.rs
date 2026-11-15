use prodrome_core::fold::{Inflationary, Order};

/// Where a decision stands, as its life register orders it: a two-point chain, stated below
/// withdrawn. Declared inflationary, so the store refuses a restatement of a withdrawn decision:
/// once withdrawn, always withdrawn, and a decision that should hold again is a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life {
    /// Stated, by a `Decided` or a `Revised`.
    Stated,
    /// Withdrawn.
    Withdrawn,
}

impl Order for Life {
    fn le(&self, other: &Self) -> bool {
        matches!((self, other), (Life::Stated, _) | (_, Life::Withdrawn))
    }
}

impl Inflationary for Life {}
