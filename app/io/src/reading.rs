//! The triage agent's chain: read as the box reads it, and never written. The agent's writes
//! are cards in the Inbox (`suzatary_core::secretary::filed::TriageCards`), so the chain it holds
//! is only a [`Reads`]: no append or accept exists on it.
//!
//! [`Reads`]: suzatary_core::store::Reads

use std::path::PathBuf;

use prodrome_core::policy::Untrusted;
use suzatary_core::record::{EventStore, TodoEvent};
use suzatary_core::store::Only;

use crate::objects::Objects;

/// One chain, read.
pub type Reading = Only<Objects<TodoEvent>>;

/// The chain at `root`, read under its own `policy`.
#[must_use]
pub fn chain(root: PathBuf, policy: Untrusted) -> Reading {
    Only::new(Objects::chain(EventStore::new(root, policy)))
}
