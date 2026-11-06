//! What the tab holds of the open chain, as a page reads it: the rows its fold shows
//! (`Local::Fold`), and what it wrote that the box has not taken (`Local::Unsent`).

use std::collections::BTreeMap;

use serde::Deserialize;
use suzatary_core::wire::Entry;
use ts_rs::TS;

/// The rows of the fold the tab shows, at the instant they were read.
#[derive(Debug, Clone, Deserialize, TS)]
pub(crate) struct Folded {
    pub(crate) entries: Vec<Entry>,
    pub(crate) at: String,
}

impl Folded {
    /// The row of `todo`.
    #[must_use]
    pub(crate) fn entry(&self, todo: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.todo == todo)
    }
}

/// Where a todo's last act this tab holds stands, when the box has not taken it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, TS)]
pub(crate) enum Unsent {
    /// On its way.
    Sending,
    /// Kept until this device is trusted.
    AwaitingTrust,
    /// Refused by the box, and kept.
    Refused,
}

/// What this tab wrote that the box has not taken.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "UnsentRead")]
pub(crate) struct Unsents {
    /// Each todo an unsent act names, and where its acts stand.
    pub(crate) todos: BTreeMap<String, Unsent>,
    /// What the box said of each object it refused, verbatim.
    pub(crate) refused: Vec<String>,
    /// How many unsent objects the refused ones hold back, themselves included.
    pub(crate) blocked: u32,
    /// How many unsent acts were written in a store the box no longer serves.
    pub(crate) stranded: u32,
    /// Every unsent object a reset would drop, by name.
    pub(crate) losses: Vec<String>,
    /// How many acts wait for this device to be trusted.
    pub(crate) waiting: u32,
}
