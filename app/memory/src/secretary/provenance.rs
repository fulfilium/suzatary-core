//! Where a fact comes from: how it was written, and the origins it stands on.
//!
//! A consolidated fact's origins are the union of its sources' origins, whatever round wrote its
//! words.
//! [`crate::secretary::two_phase::TwoPhase::provenance`] computes it from the history; nothing
//! stores it.

use std::collections::BTreeSet;

use crate::secretary::memory::{FactId, Origin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub wrote: Wrote,
    pub origins: Origins,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wrote {
    /// Remembered where it was said: the owner's turn, or the card he tapped.
    Said(Origin),
    Merged {
        from: BTreeSet<FactId>,
        /// The round the owner opened that wrote its words.
        round: Origin,
    },
}

impl Provenance {
    #[must_use]
    pub fn said(origin: Origin) -> Self {
        Self {
            origins: Origins::one(origin.clone()),
            wrote: Wrote::Said(origin),
        }
    }
}

impl Wrote {
    /// The round whose call wrote the fact, where a forgetting finds the copy of its words.
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
    #[must_use]
    pub fn one(origin: Origin) -> Self {
        Self(BTreeSet::from([origin]))
    }

    #[must_use]
    pub fn join(mut self, other: Self) -> Self {
        self.0.extend(other.0);
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &Origin> {
        self.0.iter()
    }
}
