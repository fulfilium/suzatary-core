//! A health register on disk: one document under a lock, so the mail sync, the plugin sync and a
//! migration joining at once each keep the others' results.

use suzatary_core::health::{Healths, Register};
use suzatary_core::store::StoreError;

use crate::docs::Locked;

/// The register's document and its lock.
pub type HealthFile<K> = Locked<Healths<K>>;

impl<K: Ord + Clone + Sync + 'static> Locked<Healths<K>> {
    /// Join `results` in, the read and the write one critical section, and answer what it holds
    /// now.
    ///
    /// # Errors
    /// The lock, the read or the write failing.
    pub fn join(&self, results: &Healths<K>) -> Result<Healths<K>, StoreError> {
        let joined = self.rewrite(|held| Some(held.cloned().unwrap_or_default().join(results)))?;
        Ok(joined.unwrap_or_default())
    }
}

impl<K: Ord + Clone + Send + Sync + 'static> Register<K> for Locked<Healths<K>> {
    fn healths(&self) -> Result<Healths<K>, StoreError> {
        self.get().map(Option::unwrap_or_default)
    }
}
