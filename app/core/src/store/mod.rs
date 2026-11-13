//! A store in one of three shapes: [`Prodrome`] (objects, appended), [`Log`] (lines, pushed) and
//! [`Doc`] (one value, got and set), each keyed where a store holds several; [`Reads`] and
//! [`Gets`] are the read halves of the first and the last. `suzatary-io` interprets them over
//! files; what a store holds means what its schema says.

mod doc;
mod error;
mod graph;
mod hold;
mod husk;
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/in_memory.rs"]
mod in_memory;
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/in_memory_doc.rs"]
mod in_memory_doc;
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/in_memory_keys.rs"]
mod in_memory_keys;
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

pub use doc::{Doc, Gets};
pub use error::StoreError;
pub use graph::Graph;
pub use hold::{Hold, InMemoryHold, Once};
pub use husk::{Husk, HuskObject, Husking, NotAHusk};
#[cfg(any(test, feature = "nothing"))]
pub use in_memory::InMemory;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory_doc::InMemoryDoc;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory_keys::InMemoryKeys;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory_log::InMemoryLog;
pub(crate) use knowledge::Knowledge;
pub use learnt::{Learnt, learn};
pub use log::{Decide, Edit, Log};
pub use memoised::memoised;
pub use only::Only;
pub use prodrome::{Prodrome, Reads, WriteLock, append_to, begun, left_out, linearised};
pub use recording::{recording, wrote};
pub use reentrant::Reentrant;
pub use signing::{Signer, Signing, sign};
