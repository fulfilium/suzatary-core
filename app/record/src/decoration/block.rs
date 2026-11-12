//! One todo as a block shows it: [`raise_block`] prints a [`Shown`] row as a
//! `#kind(...)[body]` call.
//!
//! `priority:` is the evaluated number, not a spec. `state:`, `resolvedAt:` and
//! `authoredBy:` come from the folds and the actor, so typst reads nothing on
//! the side.

use std::collections::BTreeMap;

use crate::record::{MarkupSource, Note, NoteSite, Source, StringSource, SubTodo};
use prodrome_core::event::{Actor, Name, TodoId};
use prodrome_core::fpl;
use prodrome_core::fpl::Outcome;
use prodrome_core::literal::Datetime;

use super::value::{Arg, Call, Value};
use super::{DecorationError, read};

/// One todo as a block says it. Not `Authored`: its `at` and `note` are never
/// printed.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    /// The todo's id — `id:`.
    pub todo: TodoId,
    /// Who wrote the content record — `authoredBy:`. A triage-authored todo is
    /// marked `(unconfirmed)` where it stands, never sunk: a mis-priced
    /// provisional todo hidden at the bottom is what the marker prevents.
    pub actor: Actor,
    /// Which constructor renders it — the callee.
    pub kind: Name,
    /// `created:`, the day the todo was authored.
    pub created: Datetime,
    /// The `[body]` block: typst markup source, opaque here.
    pub body: MarkupSource,
    /// `priority:` — the evaluated fulfillment as a ratio; `None` where the core
    /// has no number (no spec and no checklist, or a function that does not link).
    pub price: Option<f64>,
    /// The `//` run above `priority:`: the recorded reason the number is what
    /// it is, and the only place that reasoning exists.
    pub rationale: Vec<String>,
    /// `category:`, absent on `ekaTodo` (which hardcodes its own).
    pub category: Option<Name>,
    /// `waitingOn:` — string source, or empty for `none`.
    pub waiting_on: StringSource,
    /// `detail:` — markup source, empty when there is none.
    pub detail: MarkupSource,
    /// `source:` — the message a todo came out of.
    pub source: Option<Source>,
    /// `subTodos:` — the checklist. An item carries no price: the parent's
    /// number already aggregates it.
    pub subtodos: Vec<SubTodo>,
    /// The `//` runs on the other fields, and the two that belong to the block
    /// as a whole.
    pub notes: Vec<Note>,
    /// `state:`/`resolvedAt:` — the confirmed fold's outcome, which the content
    /// record never holds.
    pub outcome: Option<Outcome>,
}

/// Block key → record field, in canonical render order; every order the
/// corpus uses is a subsequence of this one.
const SOURCE_KEYS: &[(&str, &str)] = &[
    ("sender", "sender"),
    ("subject", "subject"),
    ("threadId", "thread_id"),
    ("messageId", "message_id"),
    ("date", "date"),
    ("hash", "hash"),
];

/// A date as typst spells one: its day, month and year, never a time of day.
fn date_value(date: Datetime) -> Value {
    Value::Call(Box::new(Call {
        callee: "datetime".to_owned(),
        args: vec![
            Arg::named("day", Value::Num(date.day().to_string())),
            Arg::named("month", Value::Num(date.month().to_string())),
            Arg::named("year", Value::Num(date.year().to_string())),
        ],
        trailing: Vec::new(),
        hashed: false,
        tail: Vec::new(),
    }))
}

fn source_value(source: &Source) -> Value {
    let mut args = Vec::new();
    for (key, field) in SOURCE_KEYS {
        if *field == "date" {
            args.push(Arg::named(key, date_value(source.date)));
            continue;
        }
        let text = match *field {
            "sender" => source.sender.as_str(),
            "subject" => source.subject.as_str(),
            "thread_id" => source.thread_id.as_str(),
            "message_id" => source.message_id.as_str(),
            _ => source.hash.as_str(),
        };
        // `sender` and `subject` are always written: they are what a reader
        // recognises the message by, and an absent one is worth saying.
        if !text.is_empty() || matches!(*field, "sender" | "subject") {
            args.push(Arg::named(key, Value::Str(text.to_owned())));
        }
    }
    Value::Group {
        args,
        tail: Vec::new(),
    }
}

fn subtodo_value(item: &SubTodo) -> Result<Value, DecorationError> {
    Ok(Value::Group {
        args: vec![
            Arg::named(
                "complete",
                Value::Ident(if item.done { "true" } else { "false" }.to_owned()),
            ),
            Arg::named("body", read::value(&item.body)?),
        ],
        tail: Vec::new(),
    })
}

/// A row → the block AST and the comment run above it.
///
/// `id`, `created` and `waitingOn` are always emitted: `none` says nothing is owed out loud rather
/// than by omission.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per block field; the dispatch is flat and the ORDER is the grammar, \
              so splitting it would put half of a canonical argument list in another function"
)]
pub fn raise_block(row: &Shown) -> Result<(Call, Vec<String>), DecorationError> {
    let mut notes: BTreeMap<NoteSite, Vec<String>> = BTreeMap::new();
    for note in &row.notes {
        notes.insert(note.on, note.lines.clone());
    }
    let take = |notes: &mut BTreeMap<NoteSite, Vec<String>>, site: NoteSite| {
        notes.remove(&site).unwrap_or_default()
    };

    let mut args = vec![
        Arg::explained(
            "id",
            Value::Str(row.todo.as_str().to_owned()),
            take(&mut notes, NoteSite::Todo),
        ),
        Arg::explained(
            "created",
            date_value(row.created),
            take(&mut notes, NoteSite::Created),
        ),
    ];
    if let Some(category) = &row.category {
        args.push(Arg::explained(
            "category",
            Value::Ident(format!("category.{}", category.as_str())),
            take(&mut notes, NoteSite::Category),
        ));
    }
    match row.price {
        Some(price) => args.push(Arg::explained(
            "priority",
            Value::Num(format!("{:.2}%", price * 100.0)),
            row.rationale.clone(),
        )),
        None if !row.rationale.is_empty() => {
            return Err(DecorationError(
                "a rationale with no priority to explain has nowhere to render".to_owned(),
            ));
        }
        None => {}
    }
    args.push(Arg::explained(
        "waitingOn",
        if row.waiting_on.as_str().is_empty() {
            Value::Ident("none".to_owned())
        } else {
            Value::Str(row.waiting_on.as_str().to_owned())
        },
        take(&mut notes, NoteSite::WaitingOn),
    ));
    if let Some(source) = &row.source {
        args.push(Arg::explained(
            "source",
            source_value(source),
            take(&mut notes, NoteSite::Source),
        ));
    }
    if !row.detail.as_str().is_empty() {
        args.push(Arg::explained(
            "detail",
            Value::Content(row.detail.as_str().to_owned()),
            take(&mut notes, NoteSite::Detail),
        ));
    }
    if !row.subtodos.is_empty() {
        let items = row
            .subtodos
            .iter()
            .map(|item| Ok(Arg::positional(subtodo_value(item)?)))
            .collect::<Result<Vec<_>, DecorationError>>()?;
        args.push(Arg::explained(
            "subTodos",
            Value::Group {
                args: items,
                tail: Vec::new(),
            },
            take(&mut notes, NoteSite::Subtodos),
        ));
    }
    args.push(Arg::named(
        "authoredBy",
        Value::Str(row.actor.as_str().to_owned()),
    ));
    if let Some(outcome) = row.outcome {
        let state = match outcome {
            Outcome::Completed(_) => "completed",
            Outcome::Cancelled(_) => "cancelled",
        };
        args.push(Arg::named("state", Value::Str(state.to_owned())));
        args.push(Arg::named("resolvedAt", Value::Str(fpl::iso(outcome.at()))));
    }

    let leading = take(&mut notes, NoteSite::Block);
    let tail = take(&mut notes, NoteSite::Tail);
    if !notes.is_empty() {
        let left: Vec<&str> = notes.keys().map(|site| site.as_str()).collect();
        return Err(DecorationError(format!(
            "notes on fields the row does not carry: {left:?}"
        )));
    }
    Ok((
        Call {
            callee: row.kind.as_str().to_owned(),
            args,
            trailing: vec![row.body.as_str().to_owned()],
            hashed: true,
            tail,
        },
        leading,
    ))
}
