//! What the box knows of its mail: the pushed decisions and the box-local rest, read together
//! through the dedup key. A message's decision is `pushed` at the name the key gives its hash,
//! and the hashes behind those names are `local`'s keys, so neither file alone says which
//! message was decided how, and the two read together say it all.

use crate::mail::dedup_key::DedupKey;
use crate::mail::local_state::{Kept, LocalState};
use crate::mail::state::{Decision, TriageState};

/// The two halves and the key that joins them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    /// The decisions, as `state.json` pushes them.
    pub pushed: TriageState,
    /// The rest, as the box-local tier keeps it.
    pub local: LocalState,
    /// The key the pushed names are made with.
    pub key: DedupKey,
}

impl Known {
    /// The decision recorded for the message whose canonical hash is `hash`.
    #[must_use]
    pub fn decided(&self, hash: &str) -> Option<&Decision> {
        self.pushed.decided(&self.key, hash)
    }

    /// Is this sender ignored wholesale?
    #[must_use]
    pub fn is_noise(&self, sender: &str) -> bool {
        self.local.is_noise(sender)
    }

    /// Record `decision` and `kept` for the message whose canonical hash is `hash`.
    #[must_use]
    pub fn mark(self, hash: &str, decision: Decision, kept: Kept) -> Self {
        Self {
            pushed: self.pushed.mark(&self.key, hash, decision),
            local: self.local.keep(hash, kept),
            key: self.key,
        }
    }

    /// Every decided message's hash with its decision, in hash order.
    pub fn decisions(&self) -> impl Iterator<Item = (&String, &Decision)> {
        self.local
            .messages
            .keys()
            .filter_map(|hash| Some((hash, self.decided(hash)?)))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use suzatary_base::capability::Secret;

    fn known() -> Known {
        Known {
            pushed: TriageState::empty(),
            local: LocalState::default(),
            key: DedupKey::of(&Secret::new("cd".repeat(32))).expect("a key"),
        }
    }

    proptest! {
        /// Marking reads back through the key: every message marked is decided as marked and
        /// listed by its hash, and nothing else is.
        #[test]
        fn a_mark_reads_back_through_the_key(
            hashes in prop::collection::btree_set("[0-9a-f]{64}", 0..16),
            stranger in "[0-9a-f]{64}",
        ) {
            let marked = hashes.iter().fold(known(), |known, hash| {
                known.mark(hash, Decision::Skip, Kept::default())
            });
            for hash in &hashes {
                prop_assert_eq!(marked.decided(hash), Some(&Decision::Skip));
            }
            prop_assert_eq!(marked.decided(&stranger).is_some(), hashes.contains(&stranger));
            let listed: Vec<&String> = marked.decisions().map(|(hash, _)| hash).collect();
            prop_assert_eq!(listed, hashes.iter().collect::<Vec<_>>());
        }
    }
}
