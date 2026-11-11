//! A changeset as a person deciding about it reads it: each event its acts build, as a row with
//! its before and after.
//!
//! Fields are shown as prose, not as `crate::view::authored_diff`'s canonical prints (which are
//! evidence against the bytes): a person accepting a sentence is shown the sentence, and a price
//! is the fulfillment it evaluates to, at the record's own instant (the number the writer meant)
//! under the empty environment (a proposal is not on the chain).

use std::collections::BTreeMap;

use prodrome_core::event::TodoId;
use prodrome_core::fpl;
use prodrome_core::payload::Payload as _;

use crate::believed::records;
use crate::record::{Authored, Node, TodoEvent};
use crate::wire::inbox as wire;

/// Every row of one changeset, with the "before" column advancing through it.
///
/// Rows are read in order, so a todo added and then edited shows the first row's after as the
/// second row's before.
#[must_use]
pub fn shown(nodes: &[Node], rows: &[TodoEvent]) -> Vec<wire::ProposedObject> {
    // Folded at the latest claimed instant, not a clock, so "before" does not depend on when the
    // page was opened.
    let horizon = nodes
        .iter()
        .filter_map(|node| node.event())
        .map(TodoEvent::at)
        .chain(rows.iter().map(TodoEvent::at))
        .max();
    let mut current: BTreeMap<TodoId, Authored> = horizon
        .map(|at| {
            records(nodes, at)
                .into_iter()
                .filter_map(|(todo, candidates)| Some((todo, candidates.into_iter().next()?)))
                .collect()
        })
        .unwrap_or_default();
    // Fixed before the walk, so an earlier row filling `current` cannot make a later one look
    // settled.
    let on_chain: std::collections::BTreeSet<TodoId> = current.keys().cloned().collect();
    // Filled as the walk goes: acting on a todo this same proposal adds is self-contained.
    let mut authored: std::collections::BTreeSet<TodoId> = std::collections::BTreeSet::new();
    rows.iter()
        .enumerate()
        .map(|(index, event)| {
            shown_object(index + 1, event, &mut current, &on_chain, &mut authored)
        })
        .collect()
}

/// One row, and the running "before" advanced past it.
fn shown_object(
    position: usize,
    event: &TodoEvent,
    current: &mut BTreeMap<TodoId, Authored>,
    on_chain: &std::collections::BTreeSet<TodoId>,
    authored: &mut std::collections::BTreeSet<TodoId>,
) -> wire::ProposedObject {
    let todo = event.todo().clone();
    let about = |current: &BTreeMap<TodoId, Authored>,
                 authored: &std::collections::BTreeSet<TodoId>| {
        wire::ProposedTodo {
            todo: todo.as_str().to_owned(),
            // The action line as of this object; "" when nothing has authored one.
            body: current
                .get(&todo)
                .map(|record| crate::escape::markup_text(record.payload.body.as_str()))
                .unwrap_or_default(),
            at: iso(event.at()),
            actor: event.actor().as_str().to_owned(),
            // True only for a todo that another card would put on the chain.
            pending_todo: !on_chain.contains(&todo) && !authored.contains(&todo),
        }
    };
    let act = match event {
        TodoEvent::Authored(record) => {
            let before = current.get(&todo).cloned();
            let changes = changed(before.as_ref(), record);
            current.insert(todo.clone(), (**record).clone());
            authored.insert(todo.clone());
            let todo = about(current, authored);
            if before.is_none() {
                wire::ObjectAct::Added { todo, changes }
            } else if changes
                .iter()
                .all(|c| matches!(c.field.as_str(), "priority" | "rationale"))
                && changes.iter().any(|c| c.field == "priority")
            {
                // Only the number and its reason moved: the edit `todo_update` refuses without a
                // fresh rationale.
                wire::ObjectAct::Repriced { todo, changes }
            } else {
                wire::ObjectAct::Edited { todo, changes }
            }
        }
        TodoEvent::Created(e) => wire::ObjectAct::Created {
            todo: about(current, authored),
            note: e.note.clone(),
        },
        TodoEvent::Completed(e) => wire::ObjectAct::Completed {
            todo: about(current, authored),
            note: e.note.clone(),
        },
        TodoEvent::Cancelled(e) => wire::ObjectAct::Cancelled {
            todo: about(current, authored),
            note: e.note.clone(),
        },
        TodoEvent::Reopened(e) => wire::ObjectAct::Reopened {
            todo: about(current, authored),
            note: e.note.clone(),
        },
        TodoEvent::Tended(e) => wire::ObjectAct::Tended {
            todo: about(current, authored),
            note: e.note.clone(),
        },
        TodoEvent::SpecRevised(e) => wire::ObjectAct::Respecified {
            todo: about(current, authored),
            note: e.note.clone(),
        },
    };
    wire::ProposedObject { position, act }
}

/// Every changed field a person prices or reads, before and after.
///
/// `created`, `subtodos`, `notes` and `source` are left to the todo page: they are not what the
/// todo says, or not what this surface writes.
fn changed(before: Option<&Authored>, after: &Authored) -> Vec<wire::ProposedChange> {
    /// A field's name and the total reader that says what it says.
    type Reader = (&'static str, fn(&Authored) -> String);
    let readers: [Reader; 6] = [
        ("body", |r| {
            crate::escape::markup_text(r.payload.body.as_str())
        }),
        ("detail", |r| {
            crate::escape::markup_text(r.payload.detail.as_str())
        }),
        ("priority", price),
        ("rationale", |r| {
            crate::escape::markup_text(&r.payload.rationale.join(" "))
        }),
        ("waiting_on", |r| {
            crate::view::string_text(r.payload.waiting_on.as_str())
        }),
        ("category", |r| {
            r.payload
                .category
                .as_ref()
                .map(|name| name.as_str().to_owned())
                .unwrap_or_default()
        }),
    ];
    readers
        .into_iter()
        .filter_map(|(field, read)| {
            let was = before.map(read).unwrap_or_default();
            let now = read(after);
            (was != now).then(|| wire::ProposedChange {
                field: field.to_owned(),
                before: was,
                after: now,
            })
        })
        .collect()
}

/// A record's spec as its fulfillment at its own instant under the empty environment, plus its
/// recurrence in words: before its first pass a recurrence prices as what it replaced, so a
/// number alone would show no change.
fn price(record: &Authored) -> String {
    let Some(term) = record.payload.spec() else {
        return String::new();
    };
    let now = fpl::instant_of(record.at);
    // A spec that refers to other todos has no value on its own; the diff
    // shows none rather than link against a chain it does not hold.
    let number = fpl::Closed::of(term.clone()).map(|closed| {
        fpl::fulfillment(&closed, now, &fpl::Env::new()).map_or_else(
            || "unpriced".to_owned(),
            |value| format!("{:.0}%", value * 100.0),
        )
    });
    [number, crate::authoring::recurrence_words(term, now)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; ")
}

/// An instant through `fpl::iso`, the one spelling every `at` on the wire uses.
fn iso(at: prodrome_core::literal::Datetime) -> String {
    fpl::iso(fpl::instant_of(at))
}
