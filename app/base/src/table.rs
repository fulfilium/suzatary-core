//! A tabulation of a pure function in this process's memory, the Prodrome's `memo::Cache`.

use std::sync::{Mutex, MutexGuard, PoisonError};

use prodrome_core::memo::{Cache, Finding, Key, Lookup, Lru};

/// A TABULATION of a pure function, held in this process's memory: the Prodrome's
/// [`memo::Cache`](prodrome_core::memo::Cache), at most `capacity` keys, the least recently used
/// dropped first.
///
/// It MEANS the function it tabulates (`memo f = f`, a law with entries dropped at random): no
/// entry is a source of truth, any may go, and a process that starts cold computes each again and
/// answers the same. A key that comes to hold two values is the cache's finding (the function was
/// not pure, or its key left out what it read); this table never chooses between them, it
/// computes again.
#[derive(Debug)]
pub struct Table<V>(Mutex<Cache<V, Lru>>);

impl<V: Clone + Eq> Table<V> {
    /// An empty table holding at most `capacity` keys.
    #[must_use]
    pub fn new(capacity: usize) -> Table<V> {
        Table(Mutex::new(Cache::new(Lru::new(capacity))))
    }

    /// The value kept under `key`, if one is.
    pub fn kept(&self, key: &Key) -> Option<V> {
        match self.held().get(key) {
            Lookup::Hit(value) => Some(value.clone()),
            Lookup::Miss | Lookup::Finding(_) => None,
        }
    }

    /// `value` kept under `key`, the value of the function at what `key` names.
    ///
    /// # Errors
    ///
    /// A [`Finding`] when `key` already held another value: both are held as evidence and
    /// `kept` answers nothing for the key, so the function is computed again.
    pub fn keep(&self, key: Key, value: V) -> Result<(), Finding> {
        self.held().insert(key, value)
    }

    /// Forget what `key` keeps, the value or a finding's two; forgetting what is not kept is
    /// nothing.
    pub fn forget(&self, key: &Key) {
        self.held().remove(key);
    }

    /// Every key that holds two values.
    pub fn findings(&self) -> Vec<Finding> {
        self.held().findings().collect()
    }

    /// `computed`'s value, which `key` names: the kept one, or computed now (outside the lock, so
    /// one slow computation holds up no other key) and kept.
    pub fn through(&self, key: Key, computed: impl FnOnce() -> V) -> V {
        if let Some(kept) = self.kept(&key) {
            return kept;
        }
        let value = computed();
        // A finding is held by the cache as evidence; `kept` then answers nothing for it.
        let _ = self.keep(key, value.clone());
        value
    }

    fn held(&self) -> MutexGuard<'_, Cache<V, Lru>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
