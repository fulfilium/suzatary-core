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
    begun(store, label)?;
    store.append(event)
}

/// The store's prodrome begun with a `Genesis` labelled `label` when it holds none ([`append_to`]).
///
/// # Errors
/// The core's refusal.
pub fn begun<E: Schema, Pol: Policy<E>>(
    store: &EventStore<E, Pol>,
    label: &str,
) -> Result<(), ProdromeError> {
    if store.tips()?.is_empty()
        && let Err(refused) = store.init(label)
        && store.tips()?.is_empty()
    {
        return Err(refused);
    }
    Ok(())
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

    /// The objects as one graph, which a store that memoises keeps by its heads, so a reader at
    /// heads already read reads no object.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn graph(&self) -> Result<std::sync::Arc<super::Graph<E>>, StoreError> {
        Ok(std::sync::Arc::new(super::Graph::of(self.objects()?)))
    }

    /// The print of each object `names` names as the store holds it, in that order, or `None`
    /// for one it holds no print of, read by name: whether the history holds a name is the
    /// caller's to ask of its [`Reads::graph`].
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

    /// How many bytes each object `names` names is as the store holds it, the length of its
    /// print in [`Reads::literals`], in that order, or `None` for one this store does not
    /// hold: what a content answer sends of it, and so what a reader sizes its asks by.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn sizes(&self, names: &[Hash]) -> Result<Vec<Option<usize>>, StoreError> {
        Ok(self
            .literals(names)?
            .into_iter()
            .map(|literal| literal.map(|literal| literal.len()))
            .collect())
    }

    /// The objects as the register folds read them: the graph's, shared by every reader at the
    /// same heads, never copied per call.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn nodes(&self) -> Result<std::sync::Arc<[Node<E>]>, StoreError>
    where
        E: Send + Sync + 'static,
    {
        self.graph()?.nodes()
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

    /// [`Prodrome::append`] for each event `next` yields, which runs between the writes. `next` must
    /// not read or write this store: a store on disk holds its lock and memory across the run, so
    /// such a `next` would wait on itself forever.
    ///
    /// # Errors
    /// `next`'s, or [`StoreError::Unwritable`]; what was written before stays.
    fn append_each(
        &self,
        next: &mut dyn FnMut() -> Result<Option<E>, StoreError>,
    ) -> Result<Vec<Hash>, StoreError> {
        let mut names = Vec::new();
        while let Some(event) = next()? {
            names.push(self.append(event)?);
        }
        Ok(names)
    }

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
