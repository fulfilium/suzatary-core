//! A type with finitely many values, and a set of them held as bits.

use std::marker::PhantomData;

/// A type whose every value is listed, in the order a [`Set`] iterates them.
pub trait Finite: Copy + Eq + 'static {
    /// Every value, at most 64.
    const ALL: &'static [Self];
}

/// A set of `K`'s values: a bit per value, at its place in [`Finite::ALL`].
pub struct Set<K> {
    bits: u64,
    kind: PhantomData<K>,
}

impl<K: Finite> Set<K> {
    const FITS: () = assert!(K::ALL.len() <= 64, "a Set holds at most 64 values");

    /// The set of no value.
    #[must_use]
    pub fn empty() -> Self {
        Self::of_bits(0)
    }

    fn of_bits(bits: u64) -> Self {
        Self {
            bits,
            kind: PhantomData,
        }
    }

    fn bit(value: K) -> u64 {
        let () = Self::FITS;
        K::ALL
            .iter()
            .position(|one| *one == value)
            .map_or(0, |at| 1 << at)
    }

    /// The set holding `value` alone.
    #[must_use]
    pub fn one(value: K) -> Self {
        Self::of_bits(Self::bit(value))
    }

    /// Whether it holds `value`.
    #[must_use]
    pub fn contains(&self, value: K) -> bool {
        self.bits & Self::bit(value) != 0
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Its values, in [`Finite::ALL`]'s order.
    pub fn iter(&self) -> impl Iterator<Item = K> + '_ {
        K::ALL
            .iter()
            .copied()
            .filter(move |value| self.contains(*value))
    }

    /// The union.
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        Self::of_bits(self.bits | other.bits)
    }
}

impl<K: Finite> FromIterator<K> for Set<K> {
    fn from_iter<I: IntoIterator<Item = K>>(values: I) -> Self {
        values
            .into_iter()
            .fold(Self::empty(), |set, value| set.join(&Self::one(value)))
    }
}

impl<K> Clone for Set<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for Set<K> {}

impl<K> PartialEq for Set<K> {
    fn eq(&self, other: &Self) -> bool {
        self.bits == other.bits
    }
}

impl<K> Eq for Set<K> {}

impl<K: Finite> Default for Set<K> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<K: Finite + std::fmt::Debug> std::fmt::Debug for Set<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

/// A list of its values, in [`Finite::ALL`]'s order.
impl<K: Finite + serde::Serialize> serde::Serialize for Set<K> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl<'de, K: Finite + serde::Deserialize<'de>> serde::Deserialize<'de> for Set<K> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Vec::<K>::deserialize(deserializer)?.into_iter().collect())
    }
}
