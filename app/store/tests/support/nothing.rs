use std::collections::BTreeSet;

use prodrome_core::event::{Envelope, Hash, mk_sealed, seal_hash};
use prodrome_core::schema::Schema;
use suzatary_base::capability::Nothing;

use crate::store::{Doc, Gets, Prodrome, Reads, StoreError, WriteLock};

/// A hold that holds nothing.
struct NoLock;
impl WriteLock for NoLock {}

/// Every store, inert: it holds no object, and an append is sealed and kept nowhere.
impl<E: Schema> Reads<E> for Nothing {
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError> {
        Ok(Vec::new())
    }
    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError> {
        Ok(BTreeSet::new())
    }
    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        Ok(Vec::new())
    }
    fn verify(&self) -> Vec<String> {
        Vec::new()
    }
}

impl<E: Schema> Prodrome<E> for Nothing {
    fn append(&self, event: E) -> Result<Hash, StoreError> {
        Ok(seal_hash(&mk_sealed(None, event)))
    }
    fn accept(&self, _objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        Ok(Vec::new())
    }
    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        Ok(Box::new(NoLock))
    }
}

/// No file was uploaded or kept, no fact's words either, and none is kept here.
impl<K, T> Gets<K, T> for Nothing {
    fn get(&self, _key: &K) -> Result<Option<T>, StoreError> {
        Ok(None)
    }
}

impl<K, T> Doc<K, T> for Nothing {
    fn set(&self, _key: &K, _value: &T) -> Result<(), StoreError> {
        Err(StoreError::unwritable("file store", "nothing keeps files"))
    }
    fn remove(&self, _key: &K) -> Result<(), StoreError> {
        Ok(())
    }
}
