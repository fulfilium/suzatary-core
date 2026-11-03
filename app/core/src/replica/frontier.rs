//! Where a replica stands with the box.

use prodrome_core::event::Hash;

use crate::wire::SinceRead;

/// Nothing the box named yet, or the frontier an answer named, which the next pull names back as
/// `since`. The era counts the answers that said the box no longer held the frontier asked from,
/// so a frontier named after one stands over every one named before it.
///
/// The derived order is the join's: the later era, then the larger count, then the greater
/// digest. Each frontier names only nodes the replica holding it holds, so either is safe in
/// their union, and the later is the one asked from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Frontier {
    /// No answer taken yet.
    Nothing,
    /// What an answer named.
    Named {
        /// How many answers before it said the frontier asked from was not held.
        era: u64,
        /// The nodes at or under the frontier's heads.
        count: u64,
        /// What the box recorded the frontier under.
        digest: Hash,
    },
}

impl Frontier {
    /// The later of the two.
    #[must_use]
    pub fn later(self, other: Self) -> Self {
        self.max(other)
    }

    /// The frontier an answer names, `digest` under `count` nodes, to a replica that asked from
    /// this one: an era later when the box read the frontier asked from as unknown.
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

    /// What a pull names as `since`: the digest, or none.
    #[must_use]
    pub fn digest(&self) -> Option<&Hash> {
        match self {
            Frontier::Nothing => None,
            Frontier::Named { digest, .. } => Some(digest),
        }
    }
}
