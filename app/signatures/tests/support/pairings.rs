//! A pairing held in memory: test support, never production.

use super::{Pairing, Pairings};
use suzatary_store::store::StoreError;

/// A pairing held in memory, for tests.
#[derive(Debug, Default)]
pub struct InMemoryPairings(std::sync::Mutex<Option<Pairing>>);

impl Pairings for InMemoryPairings {
    fn rewrite(
        &self,
        step: &mut dyn FnMut(Option<&Pairing>) -> Option<Pairing>,
    ) -> Result<(), StoreError> {
        let mut held = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(kept) = step(held.as_ref()) {
            *held = Some(kept);
        }
        Ok(())
    }
}
