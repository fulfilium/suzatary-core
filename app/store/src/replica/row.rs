//! A store's row: its replica and the keys of its words.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::Debug;

use crate::replica::held::{Held, Node};

/// The keys of a store's words; a name holds one key, so their union is a semilattice.
pub trait Keys: Clone + Default + PartialEq + Debug {
    type Name: Ord + Clone + Debug;
    #[must_use]
    fn union(self, other: &Self) -> Self;
    #[must_use]
    fn without(self, gone: &BTreeSet<Self::Name>) -> Self;
    fn is_empty(&self) -> bool;
}

/// A replica and the keys of its words. A later generation, then a later epoch, wins whole;
/// within both the join is a pair of unions.
#[derive(Debug, Clone, PartialEq)]
pub struct Row<N, K> {
    replica: Held<N>,
    keys: K,
}

impl<N: Node, K: Keys> Row<N, K> {
    /// The unit of the join within `generation`.
    #[must_use]
    pub fn empty(generation: u64) -> Self {
        Row {
            replica: Held::empty(generation),
            keys: K::default(),
        }
    }

    #[must_use]
    pub fn of(replica: Held<N>, keys: K) -> Self {
        Row { replica, keys }
    }

    #[must_use]
    pub fn replica(&self) -> &Held<N> {
        &self.replica
    }

    #[must_use]
    pub fn keys(&self) -> &K {
        &self.keys
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.replica.generation()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.replica.is_empty() && self.keys.is_empty()
    }

    /// A semilattice with [`Row::empty`] of generation 0 its unit.
    #[must_use]
    pub fn join(self, other: &Self) -> Self {
        let (mine, theirs) = (self.generation(), other.generation());
        if mine == theirs {
            let (a, b) = (self.replica.epoch(), other.replica.epoch());
            return match a.cmp(&b) {
                Ordering::Less => other.clone(),
                Ordering::Greater => self,
                Ordering::Equal => Row {
                    replica: self.replica.join(&other.replica),
                    keys: self.keys.union(&other.keys),
                },
            };
        }
        if mine > theirs { self } else { other.clone() }
    }

    /// What a purge leaves: the one step down that is no reset.
    #[must_use]
    pub fn without(self, gone: &BTreeSet<K::Name>) -> Self {
        Row {
            replica: self.replica,
            keys: self.keys.without(gone),
        }
    }
}
