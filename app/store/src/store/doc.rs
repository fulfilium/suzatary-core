use super::StoreError;

/// Values, one per key, read: a record holding only this reaches no write.
pub trait Gets<K, T>: Sync {
    /// The value, or `None` when none was ever set; [`StoreError::Unreadable`] when unread.
    fn get(&self, key: &K) -> Result<Option<T>, StoreError>;
}

/// Values, one per key, each replaced whole; a single document is keyed by `()`.
pub trait Doc<K, T>: Gets<K, T> {
    /// Replace the value; a reader sees the old one or the new one, never half of either.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`], leaving the old value.
    fn set(&self, key: &K, value: &T) -> Result<(), StoreError>;

    /// Delete the value; removing what is not there succeeds.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`] for a store that keeps what it holds, or a failed deletion.
    fn remove(&self, key: &K) -> Result<(), StoreError>;

    /// [`Doc::set`] of each, all durable on return.
    fn set_all(&self, values: &[(&K, &T)]) -> Result<(), StoreError> {
        values.iter().try_for_each(|(k, v)| self.set(k, v))
    }

    /// [`Doc::remove`] of each, all durable on return.
    fn remove_all(&self, keys: &[&K]) -> Result<(), StoreError> {
        keys.iter().try_for_each(|k| self.remove(k))
    }

    /// Remove what a stopped [`Doc::set_all`] staged; call only while excluding every writer.
    fn tidy(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

/// A [`Doc`] that can say which keys hold a value.
pub trait Lists<K, T>: Doc<K, T> {
    fn held(&self) -> Result<std::collections::BTreeSet<K>, StoreError>;
}
