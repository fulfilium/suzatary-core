//! The sources a program listens to at one model, by slot.

use std::collections::BTreeMap;

use crate::subpatch::SubPatch;

/// A slot is the name a program gives a source, so no two sources share one: the last set wins.
/// A source is data, equal when it asks for the same listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subs<K, V>(BTreeMap<K, V>);

impl<K, V> Default for Subs<K, V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for Subs<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(slots: I) -> Self {
        Self(slots.into_iter().collect())
    }
}

impl<K: Ord, V> Subs<K, V> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, slot: K, source: V) -> Self {
        self.0.insert(slot, source);
        self
    }

    #[must_use]
    pub fn get(&self, slot: &K) -> Option<&V> {
        self.0.get(slot)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter()
    }
}

impl<K: Ord + Clone, V: Clone + PartialEq> Subs<K, V> {
    /// What takes a host from `self` to `next`: a `Stop` for each slot that left, then a `Start`
    /// for each slot that is new or whose source changed, each in slot order. A slot whose source
    /// is equal is in neither.
    #[must_use]
    pub fn diff(&self, next: &Self) -> Vec<SubPatch<K, V>> {
        let stops = self
            .0
            .keys()
            .filter(|slot| !next.0.contains_key(*slot))
            .map(|slot| SubPatch::Stop { slot: slot.clone() });
        let starts = next
            .0
            .iter()
            .filter(|(slot, source)| self.0.get(*slot) != Some(*source))
            .map(|(slot, source)| SubPatch::Start {
                slot: slot.clone(),
                source: source.clone(),
            });
        stops.chain(starts).collect()
    }

    /// A host's listeners after `patches`, which is how a host reads them.
    pub fn apply(&mut self, patches: &[SubPatch<K, V>]) {
        for patch in patches {
            match patch {
                SubPatch::Start { slot, source } => {
                    self.0.insert(slot.clone(), source.clone());
                }
                SubPatch::Stop { slot } => {
                    self.0.remove(slot);
                }
            }
        }
    }
}
