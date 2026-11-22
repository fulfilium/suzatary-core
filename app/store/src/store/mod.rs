//! A store in one of three shapes: [`Prodrome`] (objects, appended), [`Log`] (lines, pushed) and
//! [`Doc`] (one value, got and set). `suzatary-io` interprets them over files.

#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/counted.rs"]
mod counted;
mod doc;
mod error;
mod graph;
mod hold;
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/in_memory_doc.rs"]
mod in_memory_doc;
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/in_memory_log.rs"]
mod in_memory_log;
mod knowledge;
mod learnt;
mod log;
mod memoised;
mod only;
mod prodrome;
mod recording;
mod reentrant;
mod signing;

#[cfg(any(test, feature = "nothing"))]
pub use counted::Counted;
pub use doc::{Doc, Gets, Lists};
pub use error::{Rule, Shut, StoreError};
pub use graph::Graph;
pub use hold::{Hold, InMemoryHold, Once};
#[cfg(any(test, feature = "nothing"))]
pub use in_memory_doc::InMemoryDoc;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory_log::InMemoryLog;
pub(crate) use knowledge::Knowledge;
#[cfg(any(test, feature = "nothing"))]
pub use learnt::doors;
pub use learnt::{Learnt, learn};
pub use log::{Decide, Edit, Log};
pub use memoised::memoised;
pub use only::Only;
pub use prodrome::{Prodrome, Reads, WriteLock, append_to, begun, left_out, linearised};
pub use recording::{recording, wrote};
pub use reentrant::Reentrant;
pub use signing::{Signer, Signing, sign};
