use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use super::{Doc, Gets, StoreError};

/// Documents held in memory, for tests.
#[derive(Debug)]
pub struct InMemoryDoc<K, T> {
    docs: Mutex<BTreeMap<K, T>>,
}

impl<K, T> Default for InMemoryDoc<K, T> {
    fn default() -> Self {
        Self {
            docs: Mutex::new(BTreeMap::new()),
        }
    }
}

impl<K: Ord + Clone, T: Clone> InMemoryDoc<K, T> {
    /// Every key held, sorted.
    #[must_use]
    pub fn keys(&self) -> Vec<K> {
        self.docs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }
}

impl<K, T> Gets<K, T> for InMemoryDoc<K, T>
where
    K: Ord + Clone + Send,
    T: Clone + Send,
{
    fn get(&self, key: &K) -> Result<Option<T>, StoreError> {
        Ok(self
            .docs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned())
    }
}

impl<K, T> Doc<K, T> for InMemoryDoc<K, T>
where
    K: Ord + Clone + Send,
    T: Clone + Send,
{
    fn set(&self, key: &K, value: &T) -> Result<(), StoreError> {
        self.docs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone(), value.clone());
        Ok(())
    }

    fn remove(&self, key: &K) -> Result<(), StoreError> {
        self.docs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key);
        Ok(())
    }
}
