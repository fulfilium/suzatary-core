use std::sync::{Mutex, MutexGuard, PoisonError};

use prodrome_memo::{Cache, Finding, Key, Lookup, Lru};

/// A tabulation of a pure function in memory: the Prodrome's
/// [`memo::Cache`](prodrome_memo::Cache), at most `capacity` keys, the least recently used
/// dropped first.
///
/// It means the function it tabulates (`memo f = f`): no entry is a source of truth.
/// A key that holds two values is a finding; the table computes again and never chooses.
#[derive(Debug)]
pub struct Table<V>(Mutex<Cache<V, Lru>>);

impl<V: Clone + Eq> Table<V> {
    #[must_use]
    pub fn new(capacity: usize) -> Table<V> {
        Table(Mutex::new(Cache::new(Lru::new(capacity))))
    }

    pub fn kept(&self, key: &Key) -> Option<V> {
        match self.held().get(key) {
            Lookup::Hit(value) => Some(value),
            Lookup::Miss | Lookup::Finding(_) => None,
        }
    }

    /// # Errors
    /// A [`Finding`] when `key` already held another value; `kept` then answers nothing for it.
    pub fn keep(&self, key: Key, value: V) -> Result<(), Finding> {
        self.held().insert(key, value)
    }

    /// Forget what `key` keeps, the value or a finding's two.
    pub fn forget(&self, key: &Key) {
        self.held().remove(key);
    }

    pub fn findings(&self) -> Vec<Finding> {
        self.held().findings()
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
