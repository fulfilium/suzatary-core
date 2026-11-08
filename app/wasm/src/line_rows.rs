//! The lines' rows as the tab's worker drives them: what a pull asks from, each piece the box
//! answers, its landing, a start over and a purge, each answering the delta the worker appends to
//! the device's log (bytes, a `Uint8Array`) and the conversations it moved; and the snapshot and
//! log a warm load reads back. The worker keeps no state of the rows: every question is asked of
//! the core. A husk stands for its line's print, which the worker deletes from the device in every
//! transaction that compacts the log.
//!
//! `moved` is JSON: `"All"`, or `[<conversation>, …]`.

use std::collections::BTreeSet;

use prodrome_core::event::Hash;
use suzatary_core::conversation_id::ConversationId;
use suzatary_core::replica::frontier::Frontier;
use suzatary_core::replica::row::Row;
use suzatary_core::secretary::held_replica::{Moved, Took};
use suzatary_core::secretary::key_ring::KeyRing;
use suzatary_core::secretary::line_form::{self, LineFrame, LineReplica};
use suzatary_core::secretary::locked_text::KeyName;
use suzatary_core::secretary::outlines::Parsed;
use suzatary_core::snapshot;
use suzatary_core::wire::SkeletonPayload;
use wasm_bindgen::prelude::*;

use crate::secretary::Lines;
use crate::{json, refused};

/// One move of the rows: its frame and what it moved. Free it once read.
#[wasm_bindgen]
pub struct Step {
    delta: Vec<u8>,
    moved: String,
}

impl Step {
    /// The frame `delta` and what it moved, `moved`.
    pub(crate) fn of(delta: Vec<u8>, moved: String) -> Self {
        Step { delta, moved }
    }
}

#[wasm_bindgen]
impl Step {
    /// The delta's frame, for the device's log and the other tabs.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn delta(&self) -> Vec<u8> {
        self.delta.clone()
    }

    /// `"All"`, or `[<conversation>, …]`.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn moved(&self) -> String {
        self.moved.clone()
    }
}

#[wasm_bindgen]
impl Lines {
    /// The frontier a pull names as `since`: what is held, landed and not, stands at; `""` for
    /// none.
    #[must_use]
    pub fn since(&self) -> String {
        self.0
            .rows()
            .holding()
            .replica()
            .frontier()
            .digest()
            .map_or_else(String::new, |d| d.as_str().to_owned())
    }

    /// `[<name>, …]`, sorted: the heads of what is held, landed and not.
    pub fn heads(&self) -> Result<String, JsError> {
        let heads = self.0.rows().holding().replica().read().heads();
        json(&heads.iter().map(Hash::as_str).collect::<Vec<_>>())
    }

    /// Whether the landed row has heard the box: a reading of it is a reading of what the box
    /// served, not of nothing yet.
    #[must_use]
    pub fn heard(&self) -> bool {
        self.0.rows().landed().replica().spoken().is_some()
    }

    /// Whether the box's heads `heads` (`[<name>, …]`) name a node what is held does not read.
    pub fn behind(&self, heads: &str) -> Result<bool, JsError> {
        let heads = hashes(heads, "heads")?;
        Ok(self.0.rows().holding().replica().read().behind(&heads))
    }

    /// Whether an answer of `epoch` is of a later epoch than either row heard: the store starts
    /// over before the answer is kept.
    pub fn overtaken(&self, epoch: f64) -> Result<bool, JsError> {
        Ok(self.0.rows().overtaken(whole(epoch, "epoch")?))
    }

    /// Whether an answer of `epoch` is of an earlier epoch than the word held: the box's count
    /// went back.
    pub fn went_back(&self, epoch: f64) -> Result<bool, JsError> {
        Ok(self.0.rows().went_back(whole(epoch, "epoch")?))
    }

    /// The skeleton's answer `answer` (`SkeletonPayload` JSON) kept as a piece beside the landed
    /// row, holding the keys `keys` (`{<name>: <hex>}`; the answer's own are not read), heard at
    /// `fetched_at` (ms since the Unix epoch): the delta's frame. It moves no reading.
    pub fn piece(&mut self, answer: &str, keys: &str, fetched_at: f64) -> Result<Vec<u8>, JsError> {
        let answer: SkeletonPayload =
            serde_json::from_str(answer).map_err(|e| refused(format!("answer: {e}")))?;
        let keys: std::collections::BTreeMap<KeyName, String> =
            serde_json::from_str(keys).map_err(|e| refused(format!("keys: {e}")))?;
        let delta = self
            .0
            .piece(answer, KeyRing::of(&keys), whole(fetched_at, "fetched_at")?)
            .map_err(|e| refused(e.to_string()))?;
        line_form::frame(&LineFrame {
            delta,
            husked: Vec::new(),
        })
        .map_err(|e| refused(e.to_string()))
    }

    /// The pending pieces landed.
    pub fn land(&mut self) -> Result<Step, JsError> {
        step(&self.0.land())
    }

    /// Every row reset, in the generation after both: what a later epoch of the box starts
    /// again from.
    pub fn start_over(&mut self) -> Result<Step, JsError> {
        step(&self.0.start_over())
    }

    /// The keys `gone` (`[<name>, …]`) let go from every row, and every line each key of whose
    /// words is then gone husked: a purge's.
    pub fn forget(&mut self, gone: &str) -> Result<Step, JsError> {
        let gone: BTreeSet<KeyName> =
            serde_json::from_str(gone).map_err(|e| refused(format!("gone: {e}")))?;
        step(&self.0.forget(gone))
    }

    /// Whether a line the rows read is held with words each key of which is gone: one a purge
    /// husks, though no key is owed, as on a device that took a line after its keys were erased.
    #[must_use]
    pub fn husking(&self) -> bool {
        !self.0.bare().is_empty()
    }

    /// `[<name>, …]`: every line held as its husk, whose print the device deletes.
    pub fn husks(&self) -> Result<String, JsError> {
        json(
            &self
                .0
                .kept()
                .husks
                .keys()
                .map(Hash::as_str)
                .collect::<Vec<_>>(),
        )
    }

    /// A frame another tab appended taken: `moved`. A frame of another form moves nothing.
    pub fn apply(&mut self, frame: &[u8]) -> Result<String, JsError> {
        match line_form::unframe(frame).map_err(|e| refused(e.to_string()))? {
            Some(frame) => moved(&self.0.apply(&frame)),
            None => moved(&Moved::These(BTreeSet::new())),
        }
    }

    /// The rows and the husks, whole, in the core's form: what the device keeps, and the log after
    /// it emptied.
    pub fn snapshot(&self) -> Result<Vec<u8>, JsError> {
        snapshot::snapshot(self.0.kept()).map_err(|e| refused(e.to_string()))
    }

    /// The replica replaced by the snapshot `bytes` (none for the empty one) and then each frame of
    /// `frames` taken in order, the prints held kept and each husk standing for its line: `moved`. `frames` is the log's frames one
    /// after another, each after its length as four little-endian bytes; a frame of another form
    /// is passed over. Refused, holding what it held, for a snapshot of another form or a frame
    /// that does not read.
    pub fn load(&mut self, bytes: Option<Vec<u8>>, frames: &[u8]) -> Result<String, JsError> {
        let mut kept: LineReplica = match bytes {
            None => LineReplica::default(),
            Some(bytes) => snapshot::from_snapshot(&bytes).map_err(|e| refused(e.to_string()))?,
        };
        for frame in snapshot::frames(frames).map_err(|e| refused(e.to_string()))? {
            if let Some(frame) = line_form::unframe(frame).map_err(|e| refused(e.to_string()))? {
                kept.apply(&frame);
            }
        }
        moved(&self.0.load(kept))
    }

    /// `{landed, pending, holding, landings}`, each row
    /// `{generation, epoch, box, frontier, nodes, heads, keys}`: `box` the word read
    /// (`{epoch, heads, geneses, fetchedAt}`) or `null`, `frontier` `{digest, count, era}` or
    /// `null`, `nodes` the names read and `keys` the names of the keys held, sorted.
    pub fn rows(&self) -> Result<String, JsError> {
        let rows = self.0.rows();
        json(&serde_json::json!({
            "landed": row(rows.landed()),
            "pending": row(rows.pending()),
            "holding": row(&rows.holding()),
            "landings": rows.landings(),
        }))
    }
}

fn row(row: &Row<Parsed, KeyRing>) -> serde_json::Value {
    let replica = row.replica();
    let reading = replica.read();
    serde_json::json!({
        "generation": replica.generation(),
        "epoch": replica.epoch(),
        "box": reading.word().map(|w| serde_json::json!({
            "epoch": w.epoch,
            "heads": names(&w.heads),
            "geneses": names(&w.geneses),
            "fetchedAt": w.fetched_at,
        })),
        "frontier": match replica.frontier() {
            Frontier::Nothing => serde_json::Value::Null,
            Frontier::Named { era, count, digest } => serde_json::json!({
                "digest": digest.as_str(),
                "count": count,
                "era": era,
            }),
        },
        "nodes": reading.nodes().map(|n| n.name().as_str()).collect::<Vec<_>>(),
        "heads": names(&reading.heads()),
        "keys": row.keys().iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
    })
}

fn names(set: &BTreeSet<Hash>) -> Vec<&str> {
    set.iter().map(Hash::as_str).collect()
}

fn step(took: &Took) -> Result<Step, JsError> {
    Ok(Step {
        delta: line_form::frame(&took.frame).map_err(|e| refused(e.to_string()))?,
        moved: moved(&took.moved)?,
    })
}

fn moved(moved: &Moved) -> Result<String, JsError> {
    match moved {
        Moved::All => json(&"All"),
        Moved::These(ids) => json(&ids.iter().map(ConversationId::as_str).collect::<Vec<_>>()),
    }
}

pub(crate) fn hashes(text: &str, what: &str) -> Result<BTreeSet<Hash>, JsError> {
    let names: Vec<String> =
        serde_json::from_str(text).map_err(|e| refused(format!("{what}: {e}")))?;
    names
        .into_iter()
        .map(|name| Hash::new(name).map_err(|e| refused(format!("{what}: {e}"))))
        .collect()
}

/// The largest whole number a JavaScript number holds exactly.
const SAFE: f64 = 9_007_199_254_740_991.0;

/// A JavaScript number that is a whole number from 0 to 2^53 - 1, as one.
pub(crate) fn whole(n: f64, what: &str) -> Result<u64, JsError> {
    if n.is_finite() && n.fract() == 0.0 && (0.0..=SAFE).contains(&n) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole number from 0 to 2^53 - 1, checked above"
        )]
        Ok(n as u64)
    } else {
        Err(refused(format!(
            "{what}: {n} is no whole number of 0 or more"
        )))
    }
}
