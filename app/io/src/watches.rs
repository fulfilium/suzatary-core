//! The watches on mail the owner expects: one `<id>.jsonl` per watch in the box-local tier.

use std::path::PathBuf;

use suzatary_triage::mail::watch::{Line, WatchId};

use crate::lines::{Codec, Layout, Lines};

/// The watches, one log per watch.
pub type Watches = Lines<WatchId, Line>;

impl Watches {
    /// The watches under `dir`, made on the first watch set.
    #[must_use]
    pub fn open(dir: PathBuf) -> Self {
        Lines::new(dir, &LAYOUT, &CODEC)
    }
}

const LAYOUT: Layout<WatchId> = Layout {
    what: "watch",
    store: "watch store",
    extension: "jsonl",
    stem: |id| id.as_str().to_owned(),
    key: |stem| WatchId::new(stem).ok(),
    lock: Some("watches.lock"),
    durability: crate::lines::Durability::Synced,
};

const CODEC: Codec<Line> = Codec::json("a watch line");
