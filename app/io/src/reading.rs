//! The triage agent's chain: read as the box reads it, and never written. The agent's writes
//! are cards in the Inbox (`suzatary_core::secretary::filed::TriageCards`), so the chain it holds
//! is only a [`Reads`]: no append or accept exists on it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use prodrome_core::event::Hash;
use prodrome_core::policy::Untrusted;
use suzatary_core::record::{Envelope, EventStore, Node, TodoEvent};
use suzatary_core::store::{Reads, StoreError};

use crate::objects::Objects;

/// One chain, read.
pub struct Reading {
    store: Objects<TodoEvent>,
    root: PathBuf,
}

impl std::fmt::Debug for Reading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reading")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Reading {
    /// The chain at `root`, read under its own `policy`.
    #[must_use]
    pub fn new(root: PathBuf, policy: Untrusted) -> Self {
        Self {
            store: Objects::chain(EventStore::new(root.clone(), policy)),
            root,
        }
    }
}

impl Reads<TodoEvent> for Reading {
    fn objects(&self) -> Result<Vec<(Hash, Envelope)>, StoreError> {
        self.store.objects()
    }

    fn graph(&self) -> Result<std::sync::Arc<suzatary_core::store::Graph<TodoEvent>>, StoreError> {
        self.store.graph()
    }

    fn nodes(&self) -> Result<Vec<Node>, StoreError> {
        self.store.nodes()
    }

    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        self.store.literals(names)
    }

    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError> {
        self.store.excluded()
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.store.tips()
    }

    fn verify(&self) -> Vec<String> {
        self.store.verify()
    }
}
