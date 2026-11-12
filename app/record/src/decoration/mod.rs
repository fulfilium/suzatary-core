//! A todo's record as the typst block that shows it: `#kind(fields)[body]`, the
//! fulfillment the core computed written in as a field, so typst only draws it.
//!
//! [`block::raise_block`] builds the block's call from a row, and
//! [`render_verified`] writes it only when typst's own parser reads the text
//! back as that call, so a field from attacker-controlled mail cannot alter the
//! structure. Nothing reads a block back into a record: the chain is the record.

pub mod block;
pub mod read;
pub mod value;
pub mod verified;

use std::collections::BTreeMap;

use crate::record::{Authored, Node, TodoEvent};
use prodrome_core::fpl::Outcome;

pub use block::Shown;
pub use verified::render_verified;

/// The four constructors a block may call. `decision` (something the owner is
/// asked to settle) is a kind like the others, under the same law, rather than a
/// second vocabulary beside this one.
pub const KINDS: &[&str] = &["todo", "ekaTodo", "suzateryTodo", "decision"];

/// The kinds that file under the category their name begins with (`<category>Todo`), so a
/// record of one never carries a category of its own. A stored constructor names its
/// category; the code derives it from that name rather than spelling it twice.
pub const CATEGORY_KINDS: &[&str] = &["ekaTodo"];

/// Whether `kind` takes a `category:` argument: every kind but a [`CATEGORY_KINDS`] one.
#[must_use]
pub fn takes_category(kind: &str) -> bool {
    !CATEGORY_KINDS.contains(&kind)
}

/// The category a row of `kind` with no category of its own files under.
#[must_use]
pub fn kind_category(kind: &str) -> &str {
    if CATEGORY_KINDS.contains(&kind) {
        kind.strip_suffix("Todo").unwrap_or("other")
    } else {
        "other"
    }
}

/// A row, a block, a value or a note outside the block grammar, in words; on an agent's fields
/// it is returned as text and never raised past the MCP edge.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DecorationError(pub String);

/// The content records among the objects, by the NAME an entry gives.
///
/// An entry carries a name rather than the record (reprinting every `Authored`
/// per fold is costly); this is the caller's lookup.
#[must_use]
pub fn records_of(nodes: &[Node]) -> BTreeMap<prodrome_core::event::Hash, &Authored> {
    nodes
        .iter()
        .filter_map(|node| match node.event() {
            Some(TodoEvent::Authored(record)) => Some((node.name().clone(), record.as_ref())),
            _ => None,
        })
        .collect()
}

/// The content record and the fold's answers, as one row.
///
/// The one conversion every block is shown from, so two readers cannot spell a
/// record two ways.
#[must_use]
pub fn shown_of(record: &Authored, price: Option<f64>, outcome: Option<Outcome>) -> Shown {
    Shown {
        todo: record.todo.clone(),
        actor: record.actor.clone(),
        kind: record.payload.kind.clone(),
        created: record.payload.created,
        body: record.payload.body.clone(),
        price,
        rationale: record.payload.rationale.clone(),
        category: record.payload.category.clone(),
        waiting_on: record.payload.waiting_on.clone(),
        detail: record.payload.detail.clone(),
        source: record.payload.source.clone(),
        subtodos: record.payload.subtodos.clone(),
        notes: record.payload.notes.clone(),
        outcome,
    }
}

/// The category vocabulary in its order: by stable id, not name, so a rename does not
/// reshuffle it; the one order the pages and the agent read.
#[must_use]
pub fn categories_in_order(categories: &BTreeMap<String, u32>) -> Vec<&str> {
    let mut rows: Vec<(&u32, &String)> = categories.iter().map(|(k, v)| (v, k)).collect();
    rows.sort_unstable();
    rows.into_iter().map(|(_, name)| name.as_str()).collect()
}

#[cfg(test)]
mod tests;
