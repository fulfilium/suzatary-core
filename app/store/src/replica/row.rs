//! A store's row: its replica and the keys of its words.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::Debug;

use crate::replica::held::{Held, Node};

/// The keys of a store's words, joined as a set: a name holds one key, so their union is a
/// semilattice whichever side a name comes from.
pub trait Keys: Clone + Default + PartialEq + Debug {
    /// What names one key.
    type Name: Ord + Clone + Debug;
    /// Both sides' keys.
    #[must_use]
    fn union(self, other: &Self) -> Self;
    /// These keys less `gone`.
    #[must_use]
    fn without(self, gone: &BTreeSet<Self::Name>) -> Self;
    /// Whether no key is held.
    fn is_empty(&self) -> bool;
}

/// A replica and the keys of its words, joined as a pair of unions within one generation and one
/// epoch of the box's word; a later generation, or within one a later epoch, kept whole, so the
/// join that brings in a later epoch's word drops every node and key of the epochs before.
#[derive(Debug, Clone, PartialEq)]
pub struct Row<N, K> {
    replica: Held<N>,
    keys: K,
}

impl<N: Node, K: Keys> Row<N, K> {
    /// Nothing held, in generation `generation`: the unit of the join within it.
    #[must_use]
    pub fn empty(generation: u64) -> Self {
        Row {
            replica: Held::empty(generation),
            keys: K::default(),
        }
    }

    /// `replica` beside `keys`.
    #[must_use]
    pub fn of(replica: Held<N>, keys: K) -> Self {
        Row { replica, keys }
    }

    /// The replica.
    #[must_use]
    pub fn replica(&self) -> &Held<N> {
        &self.replica
    }

    /// The keys.
    #[must_use]
    pub fn keys(&self) -> &K {
        &self.keys
    }

    /// The replica's generation.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.replica.generation()
    }

    /// Whether it holds nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.replica.is_empty() && self.keys.is_empty()
    }

    /// The join, a semilattice with [`Row::empty`] of generation 0 its unit.
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

    /// The row less the keys `gone`: what a purge leaves, the one step down that is no reset.
    #[must_use]
    pub fn without(self, gone: &BTreeSet<K::Name>) -> Self {
        Row {
            replica: self.replica,
            keys: self.keys.without(gone),
        }
    }
}
