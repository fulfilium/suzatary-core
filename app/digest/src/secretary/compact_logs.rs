//! The log a burst of compactions is shown, taken once.
//!
//! A summary written in a burst changes the view of the log, so a log read before each call
//! differs from call to call and the provider's cache keeps none of it. The compactor reads the
//! log for what its span is part of and writes nothing from it, so every call of the burst is
//! shown the log of the first.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::secretary::compact_ask::Sharing;

/// The log of each kind of reader, as the burst's first call to that kind read it.
#[derive(Debug)]
pub(crate) struct Logs<K> {
    held: BTreeMap<K, Held>,
}

#[derive(Debug)]
struct Held {
    log: Arc<[String]>,
    marked: Marked,
}

/// Whether an earlier call marked the log, so the provider holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marked {
    Not,
    Once,
}

impl<K: Ord> Default for Logs<K> {
    fn default() -> Self {
        Self {
            held: BTreeMap::new(),
        }
    }
}

impl<K: Ord> Logs<K> {
    /// The log for `key`, read by `make` at the burst's first call to it, and how the call shares
    /// it. `others` counts the calls still owed with this log. A call marks it when an earlier
    /// call did (it reads) or another is owed (it will read what this writes); else it is alone.
    pub(crate) fn of(
        &mut self,
        key: K,
        others: usize,
        make: impl FnOnce() -> Vec<String>,
    ) -> (Arc<[String]>, Sharing) {
        let held = self.held.entry(key).or_insert_with(|| Held {
            log: make().into(),
            marked: Marked::Not,
        });
        let sharing = if held.marked == Marked::Once || others > 0 {
            held.marked = Marked::Once;
            Sharing::Shared
        } else {
            Sharing::Alone
        };
        (Arc::clone(&held.log), sharing)
    }

    /// Nothing is owed, or the budget holds the next call back.
    pub(crate) fn end(&mut self) {
        self.held.clear();
    }
}
