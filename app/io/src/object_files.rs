//! Where [`crate::objects::Objects`] keeps a prodrome's objects: the Prodrome's own store, or
//! [`crate::husked::Husked`], which also keeps husks.

use std::path::Path;
use std::sync::Arc;

use prodrome_core::dag::{Dag, Finding};
use prodrome_core::event::Hash;
use prodrome_core::literal::ProdromeError;
use prodrome_core::policy::Untrusted;
use prodrome_core::schema::Schema;
use prodrome_core::store::{EventStore, Replica};
use suzatary_store::store::begun;

/// A prodrome's files, read, appended and received.
pub trait ObjectFiles<E: Schema>: Replica<E> + Send + Sync {
    fn root(&self) -> &Path;

    /// # Errors
    /// `objects/` not listed.
    fn dag(&self) -> Result<Arc<Dag<E>>, ProdromeError>;

    fn findings(&self) -> Vec<Finding>;

    /// # Errors
    /// The core's refusal, or a genesis already held.
    fn init(&self, label: &str) -> Result<Hash, ProdromeError>;

    /// Each event `next` yields written under one hold of the lock, the store begun with `label`
    /// where it holds nothing; `written` names what was written, before a refusal too.
    ///
    /// # Errors
    /// `next`'s, or the core's refusal.
    fn append_each(
        &self,
        label: &str,
        next: &mut dyn FnMut() -> Result<Option<E>, String>,
        written: &mut Vec<Hash>,
    ) -> Result<(), String>;
}

impl<E: Schema> ObjectFiles<E> for EventStore<E, Untrusted>
where
    E: Send + Sync,
    E::Vocabulary: Send + Sync,
{
    fn root(&self) -> &Path {
        EventStore::root(self)
    }

    fn dag(&self) -> Result<Arc<Dag<E>>, ProdromeError> {
        EventStore::dag(self)
    }

    fn findings(&self) -> Vec<Finding> {
        self.verify()
    }

    fn init(&self, label: &str) -> Result<Hash, ProdromeError> {
        EventStore::init(self, label)
    }

    fn append_each(
        &self,
        label: &str,
        next: &mut dyn FnMut() -> Result<Option<E>, String>,
        written: &mut Vec<Hash>,
    ) -> Result<(), String> {
        begun(self, label).map_err(|e| e.to_string())?;
        let mut decision = self.decide().map_err(|e| e.to_string())?;
        while let Some(event) = next()? {
            written.push(decision.append(event).map_err(|e| e.to_string())?);
        }
        Ok(())
    }
}
