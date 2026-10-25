//! The whole store, [`Database`], in three shapes: [`Prodrome`] (objects, appended), [`Log`]
//! (lines, pushed) and [`Doc`] (one value, got and set), each keyed where a store holds several.
//! `suzatary-io` interprets them over files; what a store's contents mean is the module that owns
//! its type.

mod database;
mod doc;
mod error;
mod hold;
#[cfg(any(test, feature = "nothing"))]
mod in_memory;
#[cfg(any(test, feature = "nothing"))]
mod in_memory_doc;
#[cfg(any(test, feature = "nothing"))]
mod in_memory_keys;
#[cfg(any(test, feature = "nothing"))]
mod in_memory_log;
mod kind;
mod knowledge;
mod learnt;
mod log;
mod memoised;
mod prodrome;
mod recording;
mod reentrant;
mod signing;

pub use database::{Area, Database};
pub use doc::Doc;
pub use error::StoreError;
pub use hold::{InMemoryHold, Once};
#[cfg(any(test, feature = "nothing"))]
pub use in_memory::InMemory;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory_doc::InMemoryDoc;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory_keys::InMemoryKeys;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory_log::InMemoryLog;
pub use kind::{Files, Hold, Kind, Store, Tier};
pub use knowledge::Knowledge;
pub use learnt::{Learnt, learn};
pub use log::{Decide, Edit, Log};
pub use memoised::memoised;
pub use prodrome::{Prodrome, Replica, WriteLock, append_to, left_out, linearised};
pub use recording::{recording, wrote};
pub use reentrant::Reentrant;
pub use signing::{Signer, Signing, sign};
