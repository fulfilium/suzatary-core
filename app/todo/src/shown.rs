//! What `todos open` and `todo show` read of a chain at an instant, and what the static pages draw
//! (`suzatary prodrome pages`): one reading each, so a printer and a page cannot disagree.

use std::collections::BTreeMap;

use prodrome_data::event::Hash;
use schemars::JsonSchema;
use serde::Serialize;
use suzatary_base::escape::{markup_text, string_text};
use suzatary_base::refusal::Refusal;
use suzatary_record::decoration::kind_category;
use suzatary_record::record::{Authored, Entry, Envelope, TodoEvent};
use suzatary_signatures::believed::one_outcome;

use crate::wire::todo::{FieldChange, Holds};

/// One open todo, as `todos open` prints it and the list page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OpenRow {
    pub id: String,
    pub priority: Priority,
    pub kind: String,
    pub category: String,
    /// The action line as plain text.
    pub body: String,
    /// What it waits on, as plain text, where it waits.
    pub waiting_on: Option<String>,
    pub authored_by: String,
    pub unconfirmed: bool,
}

/// A todo's price at the instant its entry was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Priority {
    /// Its fulfillment, `"12.34%"`.
    Percent { percent: String },
    /// `∅`: it has no function.
    Absent,
    /// Its function does not link.
    Unlinked,
}

#[must_use]
pub fn price(entry: &Entry) -> Priority {
    match entry.value() {
        Ok(Some(value)) => Priority::Percent {
            percent: percent(value),
        },
        Ok(None) => Priority::Absent,
        Err(_) => Priority::Unlinked,
    }
}

/// A fulfillment as a row prints it, `"12.34%"`.
#[must_use]
pub fn percent(value: f64) -> String {
    format!("{:.2}%", value * 100.0)
}

/// Every open todo with content beside its record, ascending in fulfillment (low is urgent), ties
/// by id, then every row with no number; the Inbox's held-batch decisions are left out, as the web
/// app leaves them out.
#[must_use]
pub fn open<'a>(
    entries: &'a [Entry],
    records: &BTreeMap<Hash, &'a Authored>,
) -> Vec<(&'a Entry, &'a Authored)> {
    let mut open: Vec<(&Entry, &Authored)> = entries
        .iter()
        .filter(|entry| one_outcome(entry.outcome()).is_none())
        .filter_map(|entry| Some((entry, *records.get(entry.content().first()?)?)))
        .filter(|(_, record)| !record.payload.is_decision())
        .collect();
    open.sort_by(|(left, _), (right, _)| prodrome_reading::view::list_order(left, right));
    open
}

#[must_use]
pub fn open_rows(entries: &[Entry], records: &BTreeMap<Hash, &Authored>) -> Vec<OpenRow> {
    open(entries, records)
        .into_iter()
        .map(|(entry, record)| open_row(entry, record))
        .collect()
}

#[must_use]
pub fn open_row(entry: &Entry, record: &Authored) -> OpenRow {
    OpenRow {
        id: entry.key.as_str().to_owned(),
        priority: price(entry),
        kind: record.payload.kind.as_str().to_owned(),
        category: record.payload.category.as_ref().map_or_else(
            || kind_category(record.payload.kind.as_str()).to_owned(),
            |category| category.as_str().to_owned(),
        ),
        body: markup_text(record.payload.body.as_str()),
        waiting_on: Some(string_text(record.payload.waiting_on.as_str()))
            .filter(|text| !text.is_empty()),
        authored_by: record.actor.as_str().to_owned(),
        unconfirmed: entry.confidence.is_provisional(),
    }
}

/// One todo whole, as `todo show` prints it and its page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Shown {
    pub todo: String,
    /// The core's spelling: `open`, `completed`, `cancelled`, `|` between the candidates of a
    /// conflict.
    pub state: String,
    pub priority: Priority,
    /// Every content candidate, by object name: one, or a conflict shown whole.
    pub candidates: Vec<Candidate>,
    /// Every object that names the todo, in causal order.
    pub history: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Candidate {
    pub object: String,
    pub kind: String,
    pub authored_by: String,
    /// Typst markup.
    pub body: String,
    /// Typst markup.
    pub detail: String,
    /// The record as its block shows it, priced, with its lifecycle.
    pub block: String,
}

/// One object of a todo's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Event {
    pub object: String,
    pub kind: String,
    pub actor: String,
    /// ISO 8601.
    pub at: String,
    pub note: String,
    /// What an `Authored` changed against the one before it, each side its canonical print.
    pub changed: Vec<FieldChange>,
}

/// `entry` whole, or `None` for a todo no record has authored; refused for a candidate the block
/// grammar cannot show.
pub fn shown(entry: &Entry, by_hash: &BTreeMap<Hash, &Envelope>) -> Result<Option<Shown>, Refusal> {
    let state = one_outcome(entry.outcome());
    let value = entry.value().ok().flatten();
    let candidates = entry
        .content()
        .iter()
        .filter_map(|name| match by_hash.get(name)?.event()? {
            TodoEvent::Authored(record) => Some((name, record)),
            _ => None,
        })
        .map(|(name, record)| {
            Ok(Candidate {
                object: name.as_str().to_owned(),
                kind: record.payload.kind.as_str().to_owned(),
                authored_by: record.actor.as_str().to_owned(),
                body: record.payload.body.as_str().to_owned(),
                detail: record.payload.detail.as_str().to_owned(),
                block: crate::todos::render_todo(record, state, value)?,
            })
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    if candidates.is_empty() {
        return Ok(None);
    }
    let mut changed = crate::view::diffs(&entry.stream, by_hash);
    let history = entry
        .stream
        .iter()
        .filter_map(|name| by_hash.get(name).map(|envelope| (name, envelope)))
        .filter_map(
            |(name, envelope)| match crate::view::link(name, envelope).holds {
                Holds::Event {
                    kind,
                    actor,
                    at,
                    note,
                    ..
                } => Some(Event {
                    object: name.as_str().to_owned(),
                    kind,
                    actor,
                    at,
                    note,
                    changed: changed.remove(name.as_str()).unwrap_or_default(),
                }),
                Holds::Envelope { .. } => None,
            },
        )
        .collect();
    Ok(Some(Shown {
        todo: entry.key.as_str().to_owned(),
        state: entry.state(),
        priority: price(entry),
        candidates,
        history,
    }))
}
