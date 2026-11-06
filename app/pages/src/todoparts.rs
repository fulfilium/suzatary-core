//! What every page says of a todo, the same in the list, the todo's page and its pane: the acts a
//! todo offers and their words, its title as words, its number as a pie or words, and the chips a
//! header or a line draws.

use serde::{Deserialize, Serialize};
use suzatary_core::wire::{Authored, Block, Entry, EntryState, Fulfillment};
use ts_rs::TS;
use viewtree::Attr::{Class, Title};
use viewtree::{Node, view};

use crate::chrome::{chip, pct, percent, pie};
use crate::runtime::{Effect, Lifecycle};

/// An act on a todo: a lifecycle event this tab's core spells, or a confirmation the box prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(rename = "TodoVerb")]
pub enum Verb {
    Done,
    Tend,
    Cancel,
    Reopen,
    /// Take an unconfirmed record as this login's own.
    Confirm,
}

/// How an act reaches the chain: an event the tab spells, or one the box prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Spelled {
    /// The tab's core spells it.
    Here(Lifecycle),
    /// The box prints it (`POST …/author`).
    ByTheBox,
}

impl Verb {
    /// Its button's words.
    #[must_use]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Verb::Done => "Done",
            Verb::Tend => "Did it",
            Verb::Cancel => "Cancel",
            Verb::Reopen => "Reopen",
            Verb::Confirm => "Confirm",
        }
    }

    /// Its button's class, beside `act`.
    #[must_use]
    pub(crate) const fn class(self) -> &'static str {
        match self {
            Verb::Done => "act act-done",
            Verb::Tend => "act act-tend",
            Verb::Cancel => "act act-cancel",
            Verb::Reopen => "act act-reopen",
            Verb::Confirm => "act act-confirm",
        }
    }

    /// What the toast says once it is done.
    #[must_use]
    pub(crate) const fn said(self) -> &'static str {
        match self {
            Verb::Done => "marked done",
            Verb::Tend => "done again",
            Verb::Cancel => "cancelled",
            Verb::Reopen => "reopened",
            Verb::Confirm => "confirmed",
        }
    }

    /// How it is spelled.
    #[must_use]
    pub(crate) const fn spelled(self) -> Spelled {
        match self {
            Verb::Done => Spelled::Here(Lifecycle::Completed),
            Verb::Tend => Spelled::Here(Lifecycle::Tended),
            Verb::Cancel => Spelled::Here(Lifecycle::Cancelled),
            Verb::Reopen => Spelled::Here(Lifecycle::Reopened),
            Verb::Confirm => Spelled::ByTheBox,
        }
    }

    /// Whether an Undo is offered once it is done: a confirmation trades actors, and the box
    /// refuses to undo a tending.
    #[must_use]
    pub(crate) const fn undoable(self) -> bool {
        match self {
            Verb::Done | Verb::Cancel | Verb::Reopen => true,
            Verb::Tend | Verb::Confirm => false,
        }
    }
}

/// Whether it is open.
#[must_use]
pub(crate) fn is_open(e: &Entry) -> bool {
    e.state == EntryState::Open
}

/// The act that finishes this round of it: a recurring todo is done again, not done.
#[must_use]
pub(crate) fn finish(e: &Entry) -> Verb {
    if e.recurring { Verb::Tend } else { Verb::Done }
}

/// Which acts apply, in the order offered. Confirm rides along whenever the box says unconfirmed,
/// since the roster lives there; the one over-offer is answered by the box.
#[must_use]
pub(crate) fn applicable(e: &Entry) -> Vec<Verb> {
    let mut acts = if is_open(e) {
        vec![finish(e), Verb::Cancel]
    } else {
        vec![Verb::Reopen]
    };
    if e.unconfirmed {
        acts.push(Verb::Confirm);
    }
    acts
}

/// One line of Typst source, only its whitespace collapsed.
#[must_use]
pub(crate) fn preview(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Its title in words: the core's reading of its markup, never rendered; the id with no record.
#[must_use]
pub(crate) fn headline(e: &Entry) -> String {
    let text = e
        .authored
        .as_ref()
        .map(|a| preview(&a.text))
        .unwrap_or_default();
    if text.is_empty() {
        e.todo.clone()
    } else {
        text
    }
}

/// The constructor a record is drawn by, as its chip prints it.
#[must_use]
pub(crate) fn kind(a: &Authored) -> &str {
    match &a.block {
        Block::Decision => "decision",
        Block::Todo { name } => name,
    }
}

/// `2 of 5 done`, or nothing without a checklist.
#[must_use]
pub(crate) fn subtodos(e: &Entry) -> Option<String> {
    let subs = e.authored.as_ref().map_or(&[][..], |a| &a.subtodos[..]);
    (!subs.is_empty()).then(|| {
        format!(
            "{} of {} done",
            subs.iter().filter(|s| s.done).count(),
            subs.len()
        )
    })
}

/// Each register two writers moved at once, by name, with its concurrent writes.
#[must_use]
pub(crate) fn conflicts(e: &Entry) -> Vec<(&str, &[String])> {
    e.conflicts
        .iter()
        .filter(|(_, hashes)| hashes.len() > 1)
        .map(|(kind, hashes)| (kind.as_str(), hashes.as_slice()))
        .collect()
}

/// A value in words: its percentage, `unpriced` for `∅`, `does not link` where it does not.
#[must_use]
pub(crate) fn worded(v: Option<Fulfillment>) -> String {
    match v {
        Some(Fulfillment::Number(v)) => pct(v),
        Some(Fulfillment::Absent(_)) => "unpriced".to_owned(),
        None => "does not link".to_owned(),
    }
}

/// A value as its pie with its number on hover, or the words for one with no number.
#[must_use]
pub(crate) fn mark<M>(v: Option<Fulfillment>) -> Node<M> {
    match v {
        Some(Fulfillment::Number(v)) => view! { Span "mark" {Title => percent(v)} [pie(v)] },
        Some(Fulfillment::Absent(_)) | None => view! { Span "mark" [chip(&worded(v), "")] },
    }
}

/// Its state as a chip: open plain, a closed one with its day.
#[must_use]
pub(crate) fn state_chip<M>(e: &Entry) -> Node<M> {
    let day = e.at.get(..10).unwrap_or_default();
    let closed = |word: &str| chip(format!("{word} {day}").trim_end(), "done");
    match e.state {
        EntryState::Open => chip("open", ""),
        EntryState::Completed => closed("completed"),
        EntryState::Cancelled => closed("cancelled"),
    }
}

/// The chips under a todo's title: its state, category, kind, provenance, conflicts and checklist.
#[must_use]
pub(crate) fn chips<M>(e: &Entry) -> Node<M> {
    let a = e.authored.as_ref();
    let category = a.map(|a| a.category.as_str()).filter(|c| !c.is_empty());
    view! { Div "cardmeta" [
        state_chip(e),
        category.map(|c| chip(c, "cat")),
        a.map(|a| chip(kind(a), "kind")),
        e.unconfirmed.then(|| chip("unconfirmed", "prov")),
        (!conflicts(e).is_empty()).then(|| chip("conflict", "prov")),
        subtodos(e).map(|s| chip(&s, "")),
    ] }
}

/// Markup a trusted actor wrote, placed as the Typst block the box renders under `key` (its
/// source shown until the HTML is held); a field with no key, as an untrusted one is given, is
/// its source, set as text.
#[must_use]
pub(crate) fn markup<M>(class: &str, key: Option<&str>, source: &str) -> Node<M> {
    match key {
        Some(key) => view! { Div {Class => class.to_owned()} [viewtree::node::block(key, source)] },
        None => view! { Div {Class => format!("{class} assource")} [source] },
    }
}

/// An act on a todo as any page asks it: an act, or its last event undone or redone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "verb")]
pub enum Acting {
    Verb(Verb),
    Undo,
    Redo,
}

/// Where an act stands: asked of the box, which prints its event, or of the replica, which
/// seals it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum Phase {
    Printed,
    Sealed,
}

/// What an answer to an act comes to.
#[derive(Debug)]
pub(crate) enum Came<H, A, S> {
    /// The box printed it: these seal it.
    Seals(Vec<Effect<H, A, S>>),
    /// The replica took it.
    Taken,
    /// It was asked already and stands asked.
    Already,
    /// It was refused, in these words.
    Refused(String),
}

/// The first step of an act on `todo`: spelled by the tab's core and sealed, or printed by
/// the box; `ask` names each phase's answer.
pub(crate) fn begin<H, A, S>(
    chain: &str,
    todo: &str,
    acting: Acting,
    note: String,
    ask: impl Fn(Phase) -> A,
) -> Effect<H, A, S> {
    let printed = |body| crate::authoring::author(ask(Phase::Printed), chain, body);
    match acting {
        Acting::Verb(verb) => match verb.spelled() {
            Spelled::Here(kind) => Effect::Act {
                ask: ask(Phase::Sealed),
                act: crate::runtime::Act::Lifecycle {
                    chain: chain.to_owned(),
                    todo: todo.to_owned(),
                    kind,
                    note,
                },
                secret: crate::runtime::Secret::None,
                told: crate::runtime::Told::Answered,
            },
            Spelled::ByTheBox => printed(suzatary_core::wire::Authoring::Confirm(
                suzatary_core::wire::ConfirmTodo {
                    todo: todo.to_owned(),
                    note,
                },
            )),
        },
        Acting::Undo => printed(suzatary_core::wire::Authoring::Undo(todo.to_owned())),
        Acting::Redo => printed(suzatary_core::wire::Authoring::Redo(todo.to_owned())),
    }
}

/// What an answer in `phase` comes to: what the box printed is sealed, each seal answered as
/// `ask` names the sealed phase.
pub(crate) fn came<H, A: Clone, S>(
    chain: &str,
    phase: Phase,
    answer: crate::runtime::Answer,
    ask: impl Fn(Phase) -> A,
) -> Came<H, A, S> {
    use crate::runtime::{Outcome, Said, answered};
    match phase {
        Phase::Printed => match crate::authoring::sealed(&ask(Phase::Sealed), chain, answer) {
            Ok((_, seals)) => Came::Seals(seals),
            Err(words) => Came::Refused(words),
        },
        Phase::Sealed => match answered::<Outcome>(answer) {
            Ok(Outcome::Already) => Came::Already,
            Ok(Outcome::Answered {
                answer: Said::Taken,
                ..
            }) => Came::Taken,
            Ok(Outcome::Answered {
                answer: Said::Refused { e } | Said::Lost { e },
                ..
            })
            | Err(e) => Came::Refused(e),
        },
    }
}

/// What a page says once an act is taken: an act that can be undone offers its Undo, answered
/// as `undo`; any other says what was done; an undo or a redo says nothing.
pub(crate) fn said<H, A, S>(acting: Acting, undo: A) -> Vec<Effect<H, A, S>> {
    match acting {
        Acting::Verb(v) if v.undoable() => vec![Effect::Offer {
            words: v.said().to_owned(),
            label: "Undo".to_owned(),
            ask: undo,
        }],
        Acting::Verb(v) => vec![Effect::Toast {
            words: v.said().to_owned(),
        }],
        Acting::Undo | Acting::Redo => Vec::new(),
    }
}
