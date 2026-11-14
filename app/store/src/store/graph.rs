use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use prodrome_core::dag::Dag;
use prodrome_core::event::{Envelope, Hash};
use prodrome_core::registers::Node;
use prodrome_core::schema::Schema;

use super::StoreError;

/// A store's objects at one set of heads, read once and shared by every answer that reads the
/// same heads: the DAG, the linearisation, the heads, the prodromes they are the objects of, and
/// every reading made of them. A store keeps it beside the heads it read, and reads again when
/// they move ([`super::Reads::graph`]).
pub struct Graph<E: Schema> {
    heads: BTreeSet<Hash>,
    geneses: BTreeSet<Hash>,
    order: Vec<Hash>,
    dag: Arc<Dag<E>>,
    names: Arc<BTreeSet<String>>,
    readings: Mutex<BTreeMap<&'static str, Arc<dyn Any + Send + Sync>>>,
}

impl<E: Schema> Graph<E> {
    /// The graph of `objects`, given in the linearisation's order. Its heads are those of these
    /// objects, never ones read beside them, so the graph and its heads always agree.
    #[must_use]
    pub fn of(objects: Vec<(Hash, Envelope<E>)>) -> Self {
        let order: Vec<Hash> = objects.iter().map(|(name, _)| name.clone()).collect();
        Self::ordered(Arc::new(objects.into_iter().collect()), order)
    }

    /// The graph of `dag` itself, in its linearisation's order: the database's own objects, shared
    /// and never copied, so a store holds its objects once beside its database.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`] for objects that rest on one another in a cycle.
    pub fn over(dag: Arc<Dag<E>>) -> Result<Self, StoreError> {
        let order = dag
            .linearise()
            .map_err(|cycle| StoreError::unreadable("store", cycle))?;
        Ok(Self::ordered(dag, order))
    }

    fn ordered(dag: Arc<Dag<E>>, order: Vec<Hash>) -> Self {
        let names = Arc::new(order.iter().map(|name| name.as_str().to_owned()).collect());
        Self {
            heads: dag.tips(),
            geneses: dag.geneses(),
            order,
            dag,
            names,
            readings: Mutex::default(),
        }
    }

    /// Every head.
    #[must_use]
    pub fn heads(&self) -> &BTreeSet<Hash> {
        &self.heads
    }

    /// The genesis of every prodrome the objects belong to.
    #[must_use]
    pub fn geneses(&self) -> &BTreeSet<Hash> {
        &self.geneses
    }

    /// The objects as a DAG.
    #[must_use]
    pub fn dag(&self) -> &Dag<E> {
        &self.dag
    }

    /// The name of every object.
    #[must_use]
    pub fn names(&self) -> &Arc<BTreeSet<String>> {
        &self.names
    }

    /// How many objects there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Every object beside its name, in the linearisation's order.
    pub fn objects(&self) -> impl Iterator<Item = (&Hash, &Envelope<E>)> {
        self.order
            .iter()
            .filter_map(|name| Some((name, self.dag.get(name)?)))
    }

    /// The objects as the register folds read them, in the linearisation's order: made once and
    /// shared by every reader of this graph.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`] for objects that rest on one another in a cycle.
    pub fn nodes(&self) -> Result<Arc<[Node<E>]>, StoreError>
    where
        E: Send + Sync + 'static,
    {
        let nodes = self.reading("nodes", |graph| {
            graph
                .dag
                .nodes()
                .map(Arc::from)
                .map_err(|cycle| cycle.to_string())
        });
        match &*nodes {
            Ok(nodes) => Ok(Arc::clone(nodes)),
            Err(cycle) => Err(StoreError::unreadable("store", cycle)),
        }
    }

    /// `read` of these objects under the name `reading`: made once, then kept as long as the
    /// graph is. `read` must be a function of the objects alone; a second type under one name is
    /// read again and replaces the first.
    pub fn reading<V: Send + Sync + 'static>(
        &self,
        reading: &'static str,
        read: impl FnOnce(&Self) -> V,
    ) -> Arc<V> {
        let kept = self
            .readings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(reading)
            .cloned()
            .and_then(|value| value.downcast::<V>().ok());
        if let Some(value) = kept {
            return value;
        }
        let value = Arc::new(read(self));
        self.readings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(reading, Arc::clone(&value) as Arc<dyn Any + Send + Sync>);
        value
    }
}

impl<E: Schema> std::fmt::Debug for Graph<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph")
            .field("heads", &self.heads)
            .field("objects", &self.order.len())
            .finish_non_exhaustive()
    }
}
