//! What the box knows of its mail: a decision is `pushed` at the name the dedup key gives its
//! hash, and the hashes are `local`'s keys, so neither file alone says which message was decided
//! how.

use crate::mail::local_state::{Kept, LocalState};
use crate::mail::state::{Decision, TriageState};
use suzatary_mail::mail::dedup_key::DedupKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub pushed: TriageState,
    pub local: LocalState,
    pub key: DedupKey,
}

impl Known {
    #[must_use]
    pub fn decided(&self, hash: &str) -> Option<&Decision> {
        self.pushed.decided(&self.key, hash)
    }

    #[must_use]
    pub fn is_noise(&self, sender: &str) -> bool {
        self.local.is_noise(sender)
    }

    #[must_use]
    pub fn mark(self, hash: &str, decision: Decision, kept: Kept) -> Self {
        Self {
            pushed: self.pushed.mark(&self.key, hash, decision),
            local: self.local.keep(hash, kept),
            key: self.key,
        }
    }

    /// In hash order.
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
