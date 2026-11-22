//! A process that must never write a store opens it inside [`Only`], which has no write trait.

use std::collections::BTreeSet;
use std::sync::Arc;

use prodrome_core::event::{Envelope, Hash};
use prodrome_core::schema::Schema;

use super::{Gets, Graph, Reads, StoreError};

pub struct Only<S>(S);

impl<S> Only<S> {
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

impl<K, T, S: Gets<K, T> + ?Sized> Gets<K, T> for &S {
    fn get(&self, key: &K) -> Result<Option<T>, StoreError> {
        (**self).get(key)
    }
}
