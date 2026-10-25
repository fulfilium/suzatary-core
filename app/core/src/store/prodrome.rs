use std::collections::BTreeSet;

use prodrome_core::event::{Envelope, Hash};
use prodrome_core::literal::ProdromeError;
use prodrome_core::policy::Policy;
use prodrome_core::registers::Node;
use prodrome_core::schema::Schema;
use prodrome_core::store::EventStore;

use super::StoreError;

/// Append `event` as the Prodrome's `Change` over its entity's heads, or nothing for a twin,
/// beginning the store's prodrome with a `Genesis` labelled `label` when it holds none. Nothing
/// joins the heads afterwards: they are derived, and no reader needs one.
///
/// Another process may begin the store between the look and the `init`, which then refuses: that
/// refusal is the store begun.
///
/// # Errors
/// The core's refusal.
pub fn append_to<E: Schema, Pol: Policy<E>>(
    store: &EventStore<E, Pol>,
    label: &str,
    event: E,
) -> Result<Hash, ProdromeError> {
    if store.tips()?.is_empty()
        && let Err(refused) = store.init(label)
        && store.tips()?.is_empty()
    {
        return Err(refused);
    }
    store.append(event)
}

/// Every name a DAG leaves out of its history, and every object resting on one.
#[must_use]
pub fn left_out<E: Schema>(dag: &prodrome_core::dag::Dag<E>) -> BTreeSet<Hash> {
    dag.excluded()
        .into_iter()
        .flat_map(|excluded| std::iter::once(excluded.name).chain(excluded.resting))
        .collect()
}

/// Every object of `store`, rehashed, in the linearisation's order.
///
/// # Errors
/// The core's refusal of a file that is not an object, a missing parent, or a cycle.
pub fn linearised<E: Schema, Pol: Policy<E>>(
    store: &EventStore<E, Pol>,
) -> Result<Vec<(Hash, Envelope<E>)>, ProdromeError> {
    let dag = store.dag()?;
    Ok(dag
        .linearise()?
        .into_iter()
        .map(|name| {
            let object = dag.objects()[&name].clone();
            (name, object)
        })
        .collect())
}

/// An exclusive hold on a store's write path for as long as the value lives.
pub trait WriteLock {}

/// Content-addressed objects of schema `E`, read: what a record that may only read holds, so
/// no write is reachable from it.
///
/// An interpreter may memoise, but never answer from a stale set of heads.
pub trait Reads<E: Schema>: Sync {
    /// Every object in the linearisation's order, each beside its name.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError>;

    /// The canonical print of each object `names` names, in that order, or `None` for one this
    /// store does not hold. A store that can read one object by its name answers without
    /// reading the rest.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        let objects: std::collections::BTreeMap<Hash, Envelope<E>> =
            self.objects()?.into_iter().collect();
        Ok(names
            .iter()
            .map(|name| {
                objects
                    .get(name)
                    .map(prodrome_core::event::canonical_envelope)
            })
            .collect())
    }

    /// The objects as the register folds read them.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn nodes(&self) -> Result<Vec<Node<E>>, StoreError> {
        prodrome_core::dag::Dag::from_iter(self.objects()?)
            .nodes()
            .map_err(|e| StoreError::unreadable("store", e))
    }

    /// The event of every object that carries one, in the same order.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn events(&self) -> Result<Vec<E>, StoreError> {
        Ok(self
            .objects()?
            .into_iter()
            .filter_map(|(_, object)| object.event().cloned())
            .collect())
    }

    /// Every name the read left out: a print that is no object, or an object resting on one
    /// (the Prodrome's `Dag::excluded`). A reading may omit them; a write that deletes or
    /// decides on the reading may not, so it asks this first.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError>;

    /// Every head, sorted; fallible because deriving them reads unverified objects.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn tips(&self) -> Result<Vec<Hash>, StoreError>;

    /// The fsck's findings, empty when healthy.
    fn verify(&self) -> Vec<String>;
}

/// Content-addressed objects of schema `E`, read, appended and never edited.
pub trait Prodrome<E: Schema>: Reads<E> {
    /// Write one event as a `Change` over its entity's heads ([`append_to`]);
    /// [`Prodrome::locked`] serialises the decision above it.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`], leaving the store unchanged.
    fn append(&self, event: E) -> Result<Hash, StoreError>;

    /// The restricted join, the Prodrome's `store::accept`: take in exactly these objects, each
    /// its name and canonical print, verified on receipt, renaming nothing; answers what was
    /// taken, parents first. Taking twice is taking once.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`], having taken nothing.
    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError>;

    /// Hold the write path until the returned guard is dropped.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`] when the hold cannot be taken.
    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError>;
}
