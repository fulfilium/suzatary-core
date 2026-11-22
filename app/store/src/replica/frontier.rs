//! Where a replica stands with the box.

use prodrome_core::event::Hash;

use crate::wire::store::SinceRead;

/// The frontier an answer named, which the next pull names back as `since`.
///
/// The derived order is the join's: the later era, then the larger count, then the greater digest.
/// Each frontier names only nodes its replica holds, so either is safe in their union, and the
/// later is the one asked from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Frontier {
    Nothing,
    Named {
        /// How many answers before it said the frontier asked from was not held.
        era: u64,
        count: u64,
        digest: Hash,
    },
}

impl Frontier {
    #[must_use]
    pub fn later(self, other: Self) -> Self {
        self.max(other)
    }

    /// The frontier an answer names: an era later when the box read the `since` as unknown.
    #[must_use]
    pub fn answered(&self, since: SinceRead, digest: Hash, count: u64) -> Self {
        let era = match self {
            Frontier::Nothing => 0,
            Frontier::Named { era, .. } => *era,
        };
        let era = match since {
            SinceRead::Known => era,
            SinceRead::Unknown => era.saturating_add(1),
        };
        Frontier::Named { era, count, digest }
    }

    #[must_use]
    pub fn digest(&self) -> Option<&Hash> {
        match self {
            Frontier::Nothing => None,
            Frontier::Named { digest, .. } => Some(digest),
        }
    }
}
