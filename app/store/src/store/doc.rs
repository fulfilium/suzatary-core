use super::StoreError;

/// Values, one per key, read: what a record that may only read holds, so no write is reachable
/// from it.
pub trait Gets<K, T>: Sync {
    /// The value, or `None` when none was ever set.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn get(&self, key: &K) -> Result<Option<T>, StoreError>;
}

/// Values, one per key, each replaced whole; a single document is keyed by `()`.
pub trait Doc<K, T>: Gets<K, T> {
    /// Replace the value; a reader sees the old one or the new one, never half of either.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`], leaving the old value.
    fn set(&self, key: &K, value: &T) -> Result<(), StoreError>;

    /// Delete the value, so no later read finds it; removing what is not there succeeds.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`] for a store that keeps what it holds, or a failed deletion.
    fn remove(&self, key: &K) -> Result<(), StoreError>;
}
