//! A store that can only be read: [`Only`] holds a store privately and implements its read halves,
//! [`Reads`] and [`Gets`], and no trait that writes. A process that must never write a store opens
//! it inside one, so no write handle to it exists anywhere in that process.

use std::collections::BTreeSet;
use std::sync::Arc;

use prodrome_core::event::{Envelope, Hash};
use prodrome_core::registers::Node;
use prodrome_core::schema::Schema;

use super::{Gets, Graph, Reads, StoreError};

/// `S`, read and never written.
pub struct Only<S>(S);

impl<S> Only<S> {
    /// `store`, from now on only read.
    #[must_use]
    pub fn new(store: S) -> Self {
        Self(store)
    }
}

impl<E: Schema, S: Reads<E>> Reads<E> for Only<S> {
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError> {
        self.0.objects()
    }

    fn graph(&self) -> Result<Arc<Graph<E>>, StoreError> {
        self.0.graph()
    }

    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        self.0.literals(names)
    }

    fn sizes(&self, names: &[Hash]) -> Result<Vec<Option<usize>>, StoreError> {
        self.0.sizes(names)
    }

    fn nodes(&self) -> Result<Vec<Node<E>>, StoreError> {
        self.0.nodes()
    }

    fn events(&self) -> Result<Vec<E>, StoreError> {
        self.0.events()
    }

    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError> {
        self.0.excluded()
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.0.tips()
    }

    fn verify(&self) -> Vec<String> {
        self.0.verify()
    }
}

impl<K, T, S: Gets<K, T>> Gets<K, T> for Only<S> {
    fn get(&self, key: &K) -> Result<Option<T>, StoreError> {
        self.0.get(key)
    }
}

/// A borrowed store reads as the store, so [`Only`] can hold a borrow.
impl<E: Schema, S: Reads<E> + ?Sized> Reads<E> for &S {
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError> {
        (**self).objects()
    }

    fn graph(&self) -> Result<Arc<Graph<E>>, StoreError> {
        (**self).graph()
    }

    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        (**self).literals(names)
    }

    fn sizes(&self, names: &[Hash]) -> Result<Vec<Option<usize>>, StoreError> {
        (**self).sizes(names)
    }

    fn nodes(&self) -> Result<Vec<Node<E>>, StoreError> {
        (**self).nodes()
    }

    fn events(&self) -> Result<Vec<E>, StoreError> {
        (**self).events()
    }

    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError> {
        (**self).excluded()
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        (**self).tips()
    }

    fn verify(&self) -> Vec<String> {
        (**self).verify()
    }
}

/// A borrowed store reads as the store, so [`Only`] can hold a borrow.
impl<K, T, S: Gets<K, T> + ?Sized> Gets<K, T> for &S {
    fn get(&self, key: &K) -> Result<Option<T>, StoreError> {
        (**self).get(key)
    }
}
