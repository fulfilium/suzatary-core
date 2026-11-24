use std::collections::BTreeSet;

use prodrome_dag::policy::Policy;
use prodrome_dag::registers::Node;
use prodrome_data::event::{Envelope, Hash};
use prodrome_data::literal::ProdromeError;
use prodrome_data::schema::Schema;
use prodrome_store::store::EventStore;

use super::StoreError;

/// Append `event` as a `Change` over its entity's heads, or nothing for a twin, beginning the
/// store with a `Genesis` labelled `label` when it holds none.
pub fn append_to<E: Schema, Pol: Policy<E>>(
    store: &EventStore<E, Pol>,
    label: &str,
    event: E,
) -> Result<Hash, ProdromeError> {
    begun(store, label)?;
    store.append(event)
}

/// Begin the store with a `Genesis` labelled `label` when it holds none.
/// Another process may begin it between the look and the `init`; that refusal is the store begun.
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
pub fn left_out<E: Schema>(dag: &prodrome_dag::dag::Dag<E>) -> BTreeSet<Hash> {
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
        .iter()
        .map(|name| {
            let object = dag.objects()[name].clone();
            (name.clone(), object)
        })
        .collect())
}

/// An exclusive hold on a store's write path for as long as the value lives.
pub trait WriteLock {}

/// Content-addressed objects of schema `E`, read; no write is reachable from it.
///
/// An interpreter may memoise, but never answer from a stale set of heads.
pub trait Reads<E: Schema>: Sync {
    /// Every object in the linearisation's order, each beside its name.
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError>;

    /// The objects as one graph, which a store that memoises keeps by its heads.
    fn graph(&self) -> Result<std::sync::Arc<super::Graph<E>>, StoreError> {
        Ok(std::sync::Arc::new(super::Graph::of(self.objects()?)))
    }

    /// The stored print of each object `names` names, in that order, or `None` for one not held.
    /// Whether the history holds a name is the caller's to ask of [`Reads::graph`].
    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        let objects: std::collections::BTreeMap<Hash, Envelope<E>> =
            self.objects()?.into_iter().collect();
        Ok(names
            .iter()
            .map(|name| {
                objects
                    .get(name)
                    .map(prodrome_data::event::canonical_envelope)
            })
            .collect())
    }

    /// The length in bytes of each print in [`Reads::literals`], or `None` for one not held.
    fn sizes(&self, names: &[Hash]) -> Result<Vec<Option<usize>>, StoreError> {
        Ok(self
            .literals(names)?
            .into_iter()
            .map(|literal| literal.map(|literal| literal.len()))
            .collect())
    }

    /// The objects as the register folds read them, shared by every reader at the same heads.
    fn nodes(&self) -> Result<std::sync::Arc<[Node<E>]>, StoreError>
    where
        E: Send + Sync + 'static,
    {
        self.graph()?.nodes()
    }

    /// The event of every object that carries one, in the same order.
    fn events(&self) -> Result<Vec<E>, StoreError> {
        Ok(self
            .objects()?
            .into_iter()
            .filter_map(|(_, object)| object.event().cloned())
            .collect())
    }

    /// Every name the read left out (the Prodrome's `Dag::excluded`).
    /// A write that deletes or decides on the reading must ask this first.
    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError>;

    /// Every head, sorted; fallible because deriving them reads unverified objects.
    fn tips(&self) -> Result<Vec<Hash>, StoreError>;

    /// The fsck's findings, empty when healthy.
    fn verify(&self) -> Vec<String>;
}

/// Content-addressed objects of schema `E`, read, appended and never edited.
pub trait Prodrome<E: Schema>: Reads<E> {
    /// Write one event as a `Change` over its entity's heads; [`Prodrome::locked`] serialises the
    /// decision above it.
    fn append(&self, event: E) -> Result<Hash, StoreError>;

    /// [`Prodrome::append`] for each event `next` yields. `next` must not read or write this
    /// store: a store on disk holds its lock across the run, so it would deadlock.
    ///
    /// # Errors
    /// What was written before the error stays.
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

    /// The Prodrome's `store::accept`: take in exactly these canonical prints, verified, renaming
    /// nothing.
    /// Answers what was taken, parents first; taking twice is taking once.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`], having taken nothing.
    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError>;

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError>;
}
