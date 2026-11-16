//! Where a fact comes from: how it was written ([`Wrote`]), and the set of [`Origin`]s it stands
//! on, which has at least one member.
//!
//! What it means: a remembered fact stands on the one origin that said it; a consolidated fact
//! stands on everything its sources stood on, so its origins are the union of theirs, whatever
//! round wrote its words. The reading ([`crate::secretary::two_phase::TwoPhase::provenance`])
//! computes it from the history, and nothing stores it.

use std::collections::BTreeSet;

use crate::secretary::memory::{FactId, Origin};

/// Where a fact comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// How it was written.
    pub wrote: Wrote,
    /// Every origin it stands on: its own, or the union of its sources'.
    pub origins: Origins,
}

/// How a fact was written, as its event says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wrote {
    /// Remembered where it was said: the owner's turn, or the card he tapped.
    Said(Origin),
    /// Consolidated from `from` in `round`.
    Merged {
        /// The facts it replaces.
        from: BTreeSet<FactId>,
        /// The round the owner opened that wrote its words.
        round: Origin,
    },
}

impl Provenance {
    /// A fact said at `origin`.
    #[must_use]
    pub fn said(origin: Origin) -> Self {
        Self {
            origins: Origins::one(origin.clone()),
            wrote: Wrote::Said(origin),
        }
    }
}

impl Wrote {
    /// The round whose call wrote the fact: where a forgetting finds the copy of its words.
    #[must_use]
    pub fn round(&self) -> &Origin {
        match self {
            Wrote::Said(origin) | Wrote::Merged { round: origin, .. } => origin,
        }
    }
}

/// A set of origins with at least one member: made of one, and grown only by union.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Origins(BTreeSet<Origin>);

impl Origins {
    /// The set of `origin` alone.
    #[must_use]
    pub fn one(origin: Origin) -> Self {
        Self(BTreeSet::from([origin]))
    }

    /// The union of both.
    #[must_use]
    pub fn join(mut self, other: Self) -> Self {
        self.0.extend(other.0);
        self
    }

    /// Its members, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Origin> {
        self.0.iter()
    }
}
