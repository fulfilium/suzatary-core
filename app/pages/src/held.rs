//! What the tab holds of the open chain, as a page reads it: the rows its fold shows
//! (`Local::Fold`), and what it wrote that the box has not taken (`Local::Unsent`).

use std::collections::BTreeMap;

use serde::Deserialize;
use suzatary_todo::wire::todo::Entry;
use ts_rs::TS;

use crate::zone::Instant;

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

    /// Whether `act`'s object is in its todo's stream. A reading the tab draws while it holds an act
    /// is folded from its own replica, which keeps the act's object until it drops it (a reset, or a
    /// store the box no longer serves), so a reading without it is one no later reading will hold.
    #[must_use]
    pub(crate) fn names(&self, act: &Own) -> bool {
        self.entry(&act.todo)
            .is_some_and(|e| e.stream.contains(&act.name))
    }

    /// Whether this reading believes `act`: it names it and its instant is not after the reading's.
    /// A reading holds the object of an act the box stamped ahead of this device's clock some time
    /// before it believes it.
    #[must_use]
    pub(crate) fn holds(&self, act: &Own) -> bool {
        self.names(act) && Instant::of(&self.at).is_some_and(|t| act.at <= t)
    }
}

/// An act this tab wrote, as its replica holds it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
pub(crate) struct Own {
    pub(crate) todo: String,
    /// Its object's name.
    pub(crate) name: String,
    /// Its event's instant, naive UTC.
    #[ts(type = "string")]
    pub(crate) at: Instant,
}

/// Where a todo's last act this tab holds stands, when the box has not taken it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, TS)]
pub(crate) enum Unsent {
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
    /// Each unsent act whose todo and instant the tab read off its object.
    pub(crate) acts: Vec<Own>,
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
