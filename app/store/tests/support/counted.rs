//! A store whose reads are counted, so a test can show that a path read no object without
//! timing it.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use prodrome_core::event::{Envelope, Hash};
use prodrome_core::schema::Schema;

use super::{Graph, Prodrome, Reads, StoreError, WriteLock};

/// `store`, counting its reads: each read of objects, prints or sizes, and each graph the store
/// built again rather than the one it kept, which is a read of the whole store.
pub struct Counted<'a, E: Schema> {
    store: &'a dyn Prodrome<E>,
    reads: AtomicUsize,
    graph: Mutex<Option<Arc<Graph<E>>>>,
}

impl<'a, E: Schema> Counted<'a, E> {
    /// `store`, no read counted yet.
    #[must_use]
    pub fn new(store: &'a dyn Prodrome<E>) -> Self {
        Self {
            store,
            reads: AtomicUsize::new(0),
            graph: Mutex::default(),
        }
    }

    /// How many reads since the last time this was asked.
    pub fn reads(&self) -> usize {
        self.reads.swap(0, Ordering::SeqCst)
    }

    fn read(&self) {
        self.reads.fetch_add(1, Ordering::SeqCst);
    }
}

impl<E: Schema> Reads<E> for Counted<'_, E>
where
    Graph<E>: Send + Sync,
{
    fn graph(&self) -> Result<Arc<Graph<E>>, StoreError> {
        let graph = self.store.graph()?;
        let mut kept = self.graph.lock().unwrap_or_else(PoisonError::into_inner);
        if !kept.as_ref().is_some_and(|kept| Arc::ptr_eq(kept, &graph)) {
            self.read();
        }
        *kept = Some(Arc::clone(&graph));
        Ok(graph)
    }

    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError> {
        self.read();
        self.store.objects()
    }

    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        self.read();
        self.store.literals(names)
    }

    fn sizes(&self, names: &[Hash]) -> Result<Vec<Option<usize>>, StoreError> {
        self.read();
        self.store.sizes(names)
    }

    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError> {
        self.store.excluded()
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.store.tips()
    }

    fn verify(&self) -> Vec<String> {
        self.store.verify()
    }
}

impl<E: Schema> Prodrome<E> for Counted<'_, E>
where
    Graph<E>: Send + Sync,
{
    fn append(&self, event: E) -> Result<Hash, StoreError> {
        self.store.append(event)
    }

    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        self.store.accept(objects)
    }

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        self.store.locked()
    }
}
