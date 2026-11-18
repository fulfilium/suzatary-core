//! Which salted words may go: a text something names and nothing live names.
//!
//! Every history that names texts answers a [`Naming`]: each text it names, and whether that
//! naming is still live (a standing fact, a card the owner may still tap). A naming is a map into
//! the two-point lattice `Past < Live`, joined pointwise, so the namings of every history join
//! into one whatever their order, and a text is dead in the join exactly when it is named and
//! live nowhere. Dead is read of the join, never per history and then united: the words of a
//! dismissed card stay while the fact it became stands.
//!
//! A naming is typed by the words its texts are, so by the store they are kept in: the facts' and
//! the digest's summaries' are two stores, and a naming of one joins only with a naming of the
//! same, so a sweep can delete a text only from the store whose histories named it.
//!
//! Whether a text is dead is not monotone (a forgetting makes one), so the sweep asks it under
//! ONE hold, the memory's (`suzatary_digest::secretary::memories::Held`), and every writer that files a
//! naming of a text into that store takes the same hold first: a fact remembered, a remember card
//! filed or tapped. A writer that names texts without it could name one the sweep has just found
//! dead. A sweep that cannot read every naming deletes nothing, since the words are box-local and
//! a wrong deletion is final.

use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;

use crate::secretary::salted_text::TextName;

/// Every text of words `T` some history names, each with whether any naming of it is live.
pub struct Naming<T>(BTreeMap<TextName, Liveness>, PhantomData<fn() -> T>);

/// How a text is named: only by what is past, or by something live. `Past < Live`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Liveness {
    /// Named only by what no longer stands: a forgotten fact, an answered card.
    Past,
    /// Named by something that stands.
    Live,
}

impl<T> Naming<T> {
    /// The least naming above both: each text at the greater of its two.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        self.0.into_iter().chain(other.0).collect()
    }

    /// The texts named and live nowhere.
    #[must_use]
    pub fn dead(&self) -> BTreeSet<TextName> {
        self.at(Liveness::Past)
    }

    /// The texts something live names.
    #[must_use]
    pub fn live(&self) -> BTreeSet<TextName> {
        self.at(Liveness::Live)
    }

    fn at(&self, liveness: Liveness) -> BTreeSet<TextName> {
        self.0
            .iter()
            .filter(|(_, named)| **named == liveness)
            .map(|(text, _)| text.clone())
            .collect()
    }
}

/// Naming nothing: the join's unit.
impl<T> Default for Naming<T> {
    fn default() -> Self {
        Self(BTreeMap::new(), PhantomData)
    }
}

impl<T> Clone for Naming<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<T> PartialEq for Naming<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Naming<T> {}

impl<T> std::fmt::Debug for Naming<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Naming").field(&self.0).finish()
    }
}

/// Each naming joined, from the empty one: a text named twice is at the greater.
impl<T> FromIterator<(TextName, Liveness)> for Naming<T> {
    fn from_iter<I: IntoIterator<Item = (TextName, Liveness)>>(namings: I) -> Self {
        let mut naming = BTreeMap::new();
        for (text, liveness) in namings {
            let at = naming.entry(text).or_insert(liveness);
            *at = (*at).max(liveness);
        }
        Self(naming, PhantomData)
    }
}

/// The texts the join of every history's naming of one store leaves dead.
#[must_use]
pub fn dead<T>(histories: impl IntoIterator<Item = Naming<T>>) -> BTreeSet<TextName> {
    histories
        .into_iter()
        .fold(Naming::default(), Naming::join)
        .dead()
}

/// The texts the join of `histories` leaves dead that `held` holds: what a sweep removes, so a
/// second sweep over what the first left finds none.
#[must_use]
pub fn gone<T>(
    histories: impl IntoIterator<Item = Naming<T>>,
    held: &BTreeSet<TextName>,
) -> BTreeSet<TextName> {
    dead(histories).intersection(held).cloned().collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn text(n: u8) -> TextName {
        TextName::new(format!("{n:064x}")).expect("a name")
    }

    /// Words of no store in particular.
    struct Any;

    fn naming() -> impl Strategy<Value = Naming<Any>> {
        proptest::collection::vec((0u8..8, any::<bool>()), 0..12).prop_map(|named| {
            named
                .into_iter()
                .map(|(n, live)| (text(n), if live { Liveness::Live } else { Liveness::Past }))
                .collect()
        })
    }

    proptest! {
        #[test]
        fn namings_form_a_semilattice_with_the_empty_naming_its_unit(
            a in naming(), b in naming(), c in naming(),
        ) {
            prop_assert_eq!(a.clone().join(b.clone()), b.clone().join(a.clone()));
            prop_assert_eq!(
                a.clone().join(b.clone()).join(c.clone()),
                a.clone().join(b.clone().join(c))
            );
            prop_assert_eq!(a.clone().join(a.clone()), a.clone());
            prop_assert_eq!(a.clone().join(Naming::default()), a);
        }

        /// A text is dead exactly when some history names it and none names it live, so what
        /// one history leaves dead another can keep, and never the other way round.
        #[test]
        fn dead_is_named_and_live_nowhere(histories in proptest::collection::vec(naming(), 0..4)) {
            let dead = dead(histories.clone());
            for n in 0..8 {
                let named = histories.iter().any(|h| h.0.contains_key(&text(n)));
                let live = histories.iter().any(|h| h.0.get(&text(n)) == Some(&Liveness::Live));
                prop_assert_eq!(dead.contains(&text(n)), named && !live);
            }
            let each: BTreeSet<TextName> = histories.iter().flat_map(Naming::dead).collect();
            prop_assert!(dead.is_subset(&each));
        }

        /// A sweep removes exactly the held texts that are dead: after it no held text is dead
        /// and every text it kept is held still, and the same sweep again removes nothing.
        #[test]
        fn a_sweep_removes_the_held_dead_and_again_nothing(
            histories in proptest::collection::vec(naming(), 0..4),
            held in proptest::collection::btree_set((0u8..10).prop_map(text), 0..10),
        ) {
            let removed = gone(histories.clone(), &held);
            let left: BTreeSet<TextName> = held.difference(&removed).cloned().collect();
            prop_assert!(removed.is_subset(&held));
            prop_assert!(left.is_disjoint(&dead(histories.clone())));
            prop_assert_eq!(&removed | &left, held);
            prop_assert!(gone(histories, &left).is_empty());
        }
    }
}
