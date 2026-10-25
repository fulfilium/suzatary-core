//! A changeset: chain acts that wait for the owner's tap, whoever proposed them, and the acts a
//! trusted writer performs at once.
//!
//! Acts are stored and events are not, because an `Authored` built at proposal time would undo
//! whatever moved on the chain since; a direct write and a tap build each act the same way.

use prodrome_core::literal::{ProdromeError, Value as LiteralValue};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::agent::priced::{PRICE_SPELLINGS, Priced};
use crate::authoring::{self, NewTodo, Writer};
use crate::believed::{Believed, Preview};
use crate::capability::{Datetime, Typeset};
use crate::chains::ChainPolicy;
use crate::literal::{Fields, Literal, Signature, arm, no_arm, record};
use crate::record::{Node, Source, TodoEvent};
use crate::refusal::{Refusal, shown as quoted};
use crate::todo::{Patch, Price, TodoFields, TodoPatch};
use crate::zone::Moment;

/// One chain act, each arm backed by an existing builder.
///
/// No `Undo`, since the last event may be the owner's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "act", rename_all = "lowercase")]
pub enum Act {
    /// Author a todo that does not exist yet.
    Add {
        /// Its id.
        todo: String,
        /// Everything else.
        #[serde(flatten)]
        fields: TodoFields,
        /// The message, the day and the parent. Left out of the JSON form, which only a model
        /// reads; the stored literal carries it.
        #[serde(skip)]
        from: Provenance,
    },
    /// Change fields on a todo that does.
    Update {
        /// Which todo.
        todo: String,
        /// What changes. Only what is named.
        #[serde(flatten)]
        fields: TodoPatch,
    },
    /// Close it.
    Complete {
        /// Which todo.
        todo: String,
        /// What happened, in a sentence. "" for none.
        note: String,
    },
    /// Drop it.
    Cancel {
        /// Which todo.
        todo: String,
        /// Why, in a sentence. "" for none.
        note: String,
    },
    /// Open it again.
    Reopen {
        /// Which todo.
        todo: String,
        /// Why, in a sentence. "" for none.
        note: String,
    },
    /// A pass at a recurring todo: done again, and still open.
    Tend {
        /// Which todo.
        todo: String,
        /// What happened, in a sentence. "" for none.
        note: String,
    },
    /// Move the number, with a fresh reason for it.
    Reprice {
        /// Which todo.
        todo: String,
        /// The new price.
        priority: Price,
        /// Why that number now.
        rationale: String,
    },
}

/// What an added todo carries beyond its fields; every part is optional data, and the default
/// is a todo created today from no message, under no parent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Provenance {
    /// The message it came from.
    pub source: Option<Source>,
    /// The day it is created on, at midnight; `None` is the day it is built.
    pub created: Option<Datetime>,
    /// The todo it is a subtodo of: the parent's price gains a reference to it in the same act.
    pub parent: Option<String>,
}

/// Which act a model's record names, and on which todo.
#[derive(Deserialize, JsonSchema)]
struct Named {
    todo: String,
    act: String,
}

/// What an `add` reads beside its price.
#[derive(Deserialize, JsonSchema)]
struct Adding {
    body: String,
    rationale: String,
    #[serde(default = "crate::todo::fields::plain_kind")]
    kind: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    detail: String,
    #[serde(default)]
    waiting_on: String,
}

/// What an `update` reads beside its price.
#[derive(Deserialize, JsonSchema)]
struct Updating {
    body: Option<String>,
    detail: Option<String>,
    detail_append: Option<String>,
    rationale: Option<String>,
    waiting_on: Option<String>,
    #[serde(default)]
    clear_waiting_on: bool,
}

/// What `complete`, `cancel` and `tend` read.
#[derive(Deserialize, JsonSchema)]
struct Noted {
    #[serde(default)]
    note: String,
}

/// What a `reprice` reads beside its price.
#[derive(Deserialize, JsonSchema)]
struct Repricing {
    rationale: String,
}

impl Act {
    /// Which todo this act is about.
    #[must_use]
    pub fn todo(&self) -> &str {
        match self {
            Act::Add { todo, .. }
            | Act::Update { todo, .. }
            | Act::Complete { todo, .. }
            | Act::Cancel { todo, .. }
            | Act::Reopen { todo, .. }
            | Act::Tend { todo, .. }
            | Act::Reprice { todo, .. } => todo,
        }
    }

    /// The word a refusal and a record line name it by.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Act::Add { .. } => "add",
            Act::Update { .. } => "update",
            Act::Complete { .. } => "complete",
            Act::Cancel { .. } => "cancel",
            Act::Reopen { .. } => "reopen",
            Act::Tend { .. } => "tend",
            Act::Reprice { .. } => "reprice",
        }
    }

    /// Reads one out of the JSON a model wrote: which act and on which todo, its price in any
    /// spelling, then only the fields that act takes.
    ///
    /// # Errors
    /// [`Refusal::bad_request`] for an unknown act, a bad field, or two prices at once.
    pub fn of(fields: &Map<String, Value>) -> Result<Self, Refusal> {
        let Named { todo, act } = crate::verb::parse(fields)?;
        let priority = crate::verb::parse::<Priced>(fields)?.price()?;
        match act.as_str() {
            "add" => {
                let Adding {
                    body,
                    rationale,
                    kind,
                    category,
                    detail,
                    waiting_on,
                } = crate::verb::parse(fields)?;
                Ok(Act::Add {
                    todo,
                    fields: TodoFields {
                        body,
                        rationale,
                        kind,
                        category,
                        price: priority.ok_or_else(|| Refusal::bad_request(PRICE_SPELLINGS))?,
                        detail,
                        waiting_on,
                    },
                    from: Provenance::default(),
                })
            }
            "update" => {
                let Updating {
                    body,
                    detail,
                    detail_append,
                    rationale,
                    waiting_on,
                    clear_waiting_on,
                } = crate::verb::parse(fields)?;
                Ok(Act::Update {
                    todo,
                    fields: TodoPatch {
                        body: body.into(),
                        detail: detail.into(),
                        detail_append: detail_append.into(),
                        rationale: rationale.into(),
                        price: priority.into(),
                        waiting_on: waiting_on.into(),
                        clear_waiting_on,
                        ..TodoPatch::default()
                    },
                })
            }
            "complete" => Ok(Act::Complete {
                todo,
                note: crate::verb::parse::<Noted>(fields)?.note,
            }),
            "cancel" => Ok(Act::Cancel {
                todo,
                note: crate::verb::parse::<Noted>(fields)?.note,
            }),
            "reopen" => Ok(Act::Reopen {
                todo,
                note: crate::verb::parse::<Noted>(fields)?.note,
            }),
            "tend" => Ok(Act::Tend {
                todo,
                note: crate::verb::parse::<Noted>(fields)?.note,
            }),
            "reprice" => {
                let priority = priority.ok_or_else(|| {
                    Refusal::bad_request(
                        "a reprice names the new price: `priority` as a percentage, the \
                         `deadline_*` fields, or a cycle (`recur_every_days` or \
                         `periodic_every_days`)",
                    )
                })?;
                Ok(Act::Reprice {
                    todo,
                    priority,
                    rationale: crate::verb::parse::<Repricing>(fields)?.rationale,
                })
            }
            other => Err(Refusal::bad_request(format!(
                "no act named {} — an act is one of add, update, complete, cancel, reopen, tend, \
                 reprice",
                quoted(other)
            ))),
        }
    }

    /// The events this act is, against `nodes`: the chain plus the changeset's earlier acts. An
    /// add under a parent is two, the child and the parent's revision.
    ///
    /// # Errors
    /// Whatever the builder refuses.
    pub fn events(
        &self,
        nodes: &[Node],
        building: &Building<'_>,
    ) -> Result<Vec<TodoEvent>, Refusal> {
        let Building {
            now,
            by,
            categories,
            untrusted,
            typeset,
        } = *building;
        let at = now.at;
        match self {
            Act::Add { todo, fields, from } => {
                if crate::todos::current(nodes, at, todo).is_some() {
                    return Err(Refusal::conflict(format!(
                        "todo {} already exists — update it instead of adding it again",
                        quoted(todo)
                    )));
                }
                let new = NewTodo {
                    todo: todo.clone(),
                    fields: fields.clone(),
                    source: from.source.clone(),
                    subtodos: Vec::new(),
                };
                let mut built = match &from.parent {
                    None => vec![authoring::build_todo(
                        &new,
                        now,
                        by.actor(),
                        categories,
                        typeset,
                    )?],
                    Some(parent) => {
                        let parent = existing(nodes, at, parent)?;
                        let believed = Believed::of(nodes, at, untrusted)
                            .map_err(|failed| Refusal::internal(failed.to_string()))?;
                        let (child, revision) = authoring::build_subtodo(
                            &new,
                            &parent,
                            &believed.linkable(),
                            now,
                            by.actor(),
                            categories,
                            typeset,
                        )?;
                        vec![child, revision]
                    }
                };
                if let (Some(day), Some(TodoEvent::Authored(child))) =
                    (from.created, built.first_mut())
                {
                    child.payload.created = day;
                }
                Ok(built)
            }
            Act::Update { todo, fields } => {
                let before = existing(nodes, at, todo)?;
                authoring::update_todo(&before, fields, now, by.actor(), categories, "", typeset)
                    .map(|event| vec![event])
            }
            Act::Reprice {
                todo,
                priority,
                rationale,
            } => {
                let before = existing(nodes, at, todo)?;
                authoring::update_todo(
                    &before,
                    &TodoPatch {
                        rationale: Patch::Set(rationale.clone()),
                        price: Patch::Set(priority.clone()),
                        ..TodoPatch::default()
                    },
                    now,
                    by.actor(),
                    categories,
                    "",
                    typeset,
                )
                .map(|event| vec![event])
            }
            Act::Complete { todo, note } => lifecycle(nodes, todo, note, |t, n| {
                prodrome_core::event::mk_completed(t, at, by.actor(), n)
            }),
            Act::Cancel { todo, note } => lifecycle(nodes, todo, note, |t, n| {
                prodrome_core::event::mk_cancelled(t, at, by.actor(), n)
            }),
            Act::Reopen { todo, note } => lifecycle(nodes, todo, note, |t, n| {
                prodrome_core::event::mk_reopened(t, at, by.actor(), n)
            }),
            Act::Tend { todo, note } => lifecycle(nodes, todo, note, |t, n| {
                prodrome_core::event::mk_tended(t, at, by.actor(), n)
            }),
        }
    }
}

/// What every act is built with: the instant and the owner's zone, who writes it, the
/// deployment's categories, the chain's trust policy and the markup door.
#[derive(Clone, Copy)]
pub struct Building<'a> {
    /// The instant, and the zone a day is read in.
    pub now: Moment,
    /// Who the events are by.
    pub by: Writer,
    /// The categories a todo may name.
    pub categories: &'a [&'a str],
    /// The chain's untrusted actors, which a subtodo's loop check folds under.
    pub untrusted: &'a prodrome_core::policy::Untrusted,
    /// Typst, which admits every body.
    pub typeset: &'a dyn Typeset,
}

/// The record an edit is against, or the refusal that says there is none.
fn existing(nodes: &[Node], at: Datetime, todo: &str) -> Result<crate::record::Authored, Refusal> {
    crate::todos::current(nodes, at, todo)
        .ok_or_else(|| Refusal::not_found(format!("no todo with id {} on the chain", quoted(todo))))
}

/// The typo guard `record` has: some event on the chain must already mention the todo.
fn known(nodes: &[Node], todo: &str) -> Result<(), Refusal> {
    if nodes
        .iter()
        .filter_map(|node| node.event())
        .any(|event| event.todo().as_str() == todo)
    {
        Ok(())
    } else {
        Err(Refusal::not_found(format!(
            "unknown todo id {} — no event on the chain mentions it",
            quoted(todo)
        )))
    }
}

/// A lifecycle act: the typo guard, then the constructor.
fn lifecycle(
    nodes: &[Node],
    todo: &str,
    note: &str,
    build: impl FnOnce(&str, &str) -> Result<TodoEvent, prodrome_core::literal::ProdromeError>,
) -> Result<Vec<TodoEvent>, Refusal> {
    known(nodes, todo)?;
    build(todo, note)
        .map(|event| vec![event])
        .map_err(|failed| Refusal::bad_request(failed.to_string()))
}

/// The acts in order, never empty, deserialisation included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawChangeset", into = "RawChangeset")]
pub struct Changeset {
    chain: String,
    acts: Vec<Act>,
}

/// The shape on a stored line, and the only thing [`Changeset`] parses from.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawChangeset {
    chain: String,
    acts: Vec<Act>,
}

/// A changeset that changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "a changeset names at least one act — add, update, complete, cancel, reopen, tend or reprice"
)]
pub struct NoActs;

impl Changeset {
    /// Build one.
    ///
    /// # Errors
    /// [`NoActs`] for an empty list.
    pub fn new(chain: impl Into<String>, acts: Vec<Act>) -> Result<Self, NoActs> {
        if acts.is_empty() {
            return Err(NoActs);
        }
        Ok(Self {
            chain: chain.into(),
            acts,
        })
    }

    /// The one chain these acts are against, named when proposed.
    #[must_use]
    pub fn chain(&self) -> &str {
        &self.chain
    }

    /// The acts. Never empty.
    #[must_use]
    pub fn acts(&self) -> &[Act] {
        &self.acts
    }
}

impl TryFrom<RawChangeset> for Changeset {
    type Error = NoActs;
    fn try_from(raw: RawChangeset) -> Result<Self, NoActs> {
        Self::new(raw.chain, raw.acts)
    }
}

impl From<Changeset> for RawChangeset {
    fn from(changeset: Changeset) -> RawChangeset {
        RawChangeset {
            chain: changeset.chain,
            acts: changeset.acts,
        }
    }
}

/// Every act as its events, the one build pass the diff, the proposal check and the tap share.
///
/// # Errors
/// The first [`Refusal`], naming the act by position and by what it is.
pub fn events_of(
    changeset: &Changeset,
    chain: &Preview,
    building: &Building<'_>,
) -> Result<Vec<TodoEvent>, Refusal> {
    let mut preview = chain.clone();
    let mut built = Vec::with_capacity(changeset.acts().len());
    for (index, act) in changeset.acts().iter().enumerate() {
        for event in act
            .events(preview.nodes(), building)
            .map_err(|failed| named(index, act, &failed))?
        {
            preview
                .ahead(event.clone())
                .map_err(|failed| named(index, act, &Refusal::internal(failed.to_string())))?;
            built.push(event);
        }
    }
    Ok(built)
}

/// One act's refusal, with the act's own name in front of it.
fn named(index: usize, act: &Act, failed: &Refusal) -> Refusal {
    Refusal::new(
        failed.status(),
        format!(
            "act {} ({} {}) was refused, so nothing was appended: {}",
            index + 1,
            act.word(),
            act.todo(),
            failed.message()
        ),
    )
}

/// Why a changeset did not land, by whether anything did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unapplied {
    /// Refused before the first append: nothing is on the chain.
    Refused(Refusal),
    /// An append failed: the events before it are on the chain, and the one that failed may be.
    Appending(Refusal),
}

/// Whether `changeset` would build against `chain` as it stands: every act through the builders
/// a tap uses, each body through the markup door, nothing appended. A proposed changeset the
/// chain would refuse is refused while it is fixable, whoever proposed it.
///
/// # Errors
/// The chain's refusal of the first act it would not build, or a chain that cannot be read.
pub fn checked(
    changeset: &Changeset,
    chain: &dyn crate::store::Reads<TodoEvent>,
    building: &Building<'_>,
) -> Result<(), Refusal> {
    let preview =
        Preview::of(chain.objects()?).map_err(|failed| Refusal::internal(failed.to_string()))?;
    events_of(changeset, &preview, building).map(|_| ())
}

/// Applies a changeset under the write lock, building and verifying every act before any append.
///
/// Not a transaction: an append that fails leaves the earlier ones, and the refusal says how many.
///
/// # Errors
/// [`Unapplied::Refused`] before the first append, [`Unapplied::Appending`] after it.
pub fn apply(
    caps: &crate::capability::Secretary<'_>,
    policy: &ChainPolicy,
    categories: &[&str],
    zone: crate::zone::Zone,
    changeset: &Changeset,
    by: Writer,
) -> Result<Vec<prodrome_core::event::Hash>, Unapplied> {
    if changeset.chain() != policy.name.as_str() {
        return Err(Unapplied::Refused(Refusal::internal(format!(
            "this changeset is against {}, and this is {}",
            quoted(changeset.chain()),
            policy.name.as_str()
        ))));
    }
    let _held = caps
        .chain
        .locked()
        .map_err(|failed| Unapplied::Refused(Refusal::internal(failed.to_string())))?;
    let at = caps.clock.now();
    let chain = caps
        .chain
        .objects()
        .map_err(|failed| Refusal::internal(failed.to_string()))
        .and_then(|objects| {
            Preview::of(objects).map_err(|failed| Refusal::internal(failed.to_string()))
        })
        .map_err(Unapplied::Refused)?;
    let built = events_of(
        changeset,
        &chain,
        &Building {
            now: Moment { at, zone },
            by,
            categories,
            untrusted: &policy.untrusted,
            typeset: caps.render,
        },
    )
    .map_err(Unapplied::Refused)?;

    // Rendered and read back before anything lands.
    let mut preview = chain;
    for event in &built {
        preview
            .ahead(event.clone())
            .map_err(|failed| Unapplied::Refused(Refusal::internal(failed.to_string())))?;
        if let TodoEvent::Authored(record) = event {
            crate::todos::render_current(preview.nodes(), at, &policy.untrusted, record)
                .map_err(Unapplied::Refused)?;
        }
    }

    let mut names = Vec::with_capacity(built.len());
    for (index, event) in built.into_iter().enumerate() {
        let name = caps.chain.append(event).map_err(|failed| {
            Unapplied::Appending(Refusal::internal(format!(
                "event {} of this changeset could not be appended ({failed}) — {} event(s) \
                 before it are on the chain",
                index + 1,
                index
            )))
        })?;
        names.push(name);
    }
    Ok(names)
}

/// The changeset drawn as rows of a diff, or the refusal of an act that no longer builds.
#[must_use]
pub fn shown(
    changeset: &Changeset,
    chain: &Preview,
    building: &Building<'_>,
) -> (Vec<crate::wire::ProposedObject>, String) {
    match events_of(changeset, chain, building) {
        Ok(built) => (
            crate::secretary::diff::shown(chain.nodes(), &built),
            String::new(),
        ),
        Err(refusal) => (Vec::new(), refusal.message().to_owned()),
    }
}

/// The `acts` argument: an array of records, since a flat string would be a second grammar.
///
/// # Errors
/// [`Refusal::bad_request`] when it is not an array of objects that are acts.
pub fn acts_of(acts: Option<&Value>) -> Result<Vec<Act>, Refusal> {
    let name = "acts";
    let Some(Value::Array(listed)) = acts else {
        return Err(Refusal::bad_request(format!(
            "{name} must be an array of acts, each one an object with an `act` of add, update, \
             complete, cancel, tend or reprice"
        )));
    };
    listed
        .iter()
        .enumerate()
        .map(|(index, one)| {
            let Value::Object(fields) = one else {
                return Err(Refusal::bad_request(format!(
                    "{name}[{index}] must be an object with an `act` field"
                )));
            };
            Act::of(fields).map_err(|failed| {
                Refusal::new(
                    failed.status(),
                    format!("act {}: {}", index + 1, failed.message()),
                )
            })
        })
        .collect()
}

/// `Changes(chain, acts)`.
pub const CHANGES: Signature = ("Changes", &["chain", "acts"]);
/// `Add(todo, ...)`: a new todo's fields, its price under `priority`.
pub const ADD: Signature = (
    "Add",
    &[
        "todo",
        "body",
        "rationale",
        "kind",
        "category",
        "priority",
        "detail",
        "waiting_on",
    ],
);
/// `AddFrom(todo, ..., source, created, parent)`: an add with a [`Provenance`]. A shipped kind's
/// fields are frozen, so an add that carries one prints as a kind of its own and every `Add`
/// already stored reads as it did.
pub const ADD_FROM: Signature = (
    "AddFrom",
    &[
        "todo",
        "body",
        "rationale",
        "kind",
        "category",
        "priority",
        "detail",
        "waiting_on",
        "source",
        "created",
        "parent",
    ],
);
/// `Update(todo, ...)`: each field `None` where it is kept.
pub const UPDATE: Signature = (
    "Update",
    &[
        "todo",
        "body",
        "detail",
        "detail_append",
        "rationale",
        "priority",
        "waiting_on",
        "clear_waiting_on",
        "category",
        "subtodos",
        "defer_to",
    ],
);
/// `Complete(todo, note)`.
pub const COMPLETE: Signature = ("Complete", &["todo", "note"]);
/// `Cancel(todo, note)`.
pub const CANCEL: Signature = ("Cancel", &["todo", "note"]);
/// `Reopen(todo, note)`.
pub const REOPEN: Signature = ("Reopen", &["todo", "note"]);
/// `Tend(todo, note)`.
pub const TEND: Signature = ("Tend", &["todo", "note"]);
/// `Reprice(todo, priority, rationale)`.
pub const REPRICE: Signature = ("Reprice", &["todo", "priority", "rationale"]);

impl Literal for Act {
    fn to_value(&self) -> LiteralValue {
        let noted = |signature, todo: &String, note: &String| {
            record(signature, vec![todo.to_value(), note.to_value()])
        };
        match self {
            Act::Add { todo, fields, from } => {
                let mut values = vec![
                    todo.to_value(),
                    fields.body.to_value(),
                    fields.rationale.to_value(),
                    fields.kind.to_value(),
                    fields.category.to_value(),
                    fields.price.to_value(),
                    fields.detail.to_value(),
                    fields.waiting_on.to_value(),
                ];
                if *from == Provenance::default() {
                    record(ADD, values)
                } else {
                    values.extend([
                        from.source.to_value(),
                        from.created.to_value(),
                        from.parent.to_value(),
                    ]);
                    record(ADD_FROM, values)
                }
            }
            Act::Update { todo, fields } => record(
                UPDATE,
                vec![
                    todo.to_value(),
                    fields.body.to_value(),
                    fields.detail.to_value(),
                    fields.detail_append.to_value(),
                    fields.rationale.to_value(),
                    fields.price.to_value(),
                    fields.waiting_on.to_value(),
                    fields.clear_waiting_on.to_value(),
                    fields.category.to_value(),
                    fields.subtodos.to_value(),
                    fields.defer_to.to_value(),
                ],
            ),
            Act::Complete { todo, note } => noted(COMPLETE, todo, note),
            Act::Cancel { todo, note } => noted(CANCEL, todo, note),
            Act::Reopen { todo, note } => noted(REOPEN, todo, note),
            Act::Tend { todo, note } => noted(TEND, todo, note),
            Act::Reprice {
                todo,
                priority,
                rationale,
            } => record(
                REPRICE,
                vec![todo.to_value(), priority.to_value(), rationale.to_value()],
            ),
        }
    }

    fn from_value(value: &LiteralValue) -> Result<Self, ProdromeError> {
        let noted = |signature| -> Result<(String, String), ProdromeError> {
            let fields = Fields::of(value, signature)?;
            Ok((fields.get("todo")?, fields.get("note")?))
        };
        let added = |fields: &Fields<'_>, from| -> Result<Act, ProdromeError> {
            Ok(Act::Add {
                todo: fields.get("todo")?,
                fields: TodoFields {
                    body: fields.get("body")?,
                    rationale: fields.get("rationale")?,
                    kind: fields.get("kind")?,
                    category: fields.get("category")?,
                    price: fields.get("priority")?,
                    detail: fields.get("detail")?,
                    waiting_on: fields.get("waiting_on")?,
                },
                from,
            })
        };
        Ok(match arm(value)? {
            "Add" => added(&Fields::of(value, ADD)?, Provenance::default())?,
            "AddFrom" => {
                let fields = Fields::of(value, ADD_FROM)?;
                let from = Provenance {
                    source: fields.get("source")?,
                    created: fields.get("created")?,
                    parent: fields.get("parent")?,
                };
                added(&fields, from)?
            }
            "Update" => {
                let fields = Fields::of(value, UPDATE)?;
                Act::Update {
                    todo: fields.get("todo")?,
                    fields: TodoPatch {
                        body: fields.get("body")?,
                        detail: fields.get("detail")?,
                        detail_append: fields.get("detail_append")?,
                        rationale: fields.get("rationale")?,
                        price: fields.get("priority")?,
                        waiting_on: fields.get("waiting_on")?,
                        clear_waiting_on: fields.get("clear_waiting_on")?,
                        category: fields.get("category")?,
                        subtodos: fields.get("subtodos")?,
                        defer_to: fields.get("defer_to")?,
                    },
                }
            }
            "Complete" => {
                let (todo, note) = noted(COMPLETE)?;
                Act::Complete { todo, note }
            }
            "Cancel" => {
                let (todo, note) = noted(CANCEL)?;
                Act::Cancel { todo, note }
            }
            "Reopen" => {
                let (todo, note) = noted(REOPEN)?;
                Act::Reopen { todo, note }
            }
            "Tend" => {
                let (todo, note) = noted(TEND)?;
                Act::Tend { todo, note }
            }
            "Reprice" => {
                let fields = Fields::of(value, REPRICE)?;
                Act::Reprice {
                    todo: fields.get("todo")?,
                    priority: fields.get("priority")?,
                    rationale: fields.get("rationale")?,
                }
            }
            other => return Err(no_arm("a chain act", other)),
        })
    }
}

impl Literal for Changeset {
    fn to_value(&self) -> LiteralValue {
        record(CHANGES, vec![self.chain.to_value(), self.acts.to_value()])
    }

    fn from_value(value: &LiteralValue) -> Result<Self, ProdromeError> {
        let fields = Fields::of(value, CHANGES)?;
        Changeset::new(fields.get::<String>("chain")?, fields.get("acts")?)
            .map_err(|failed| ProdromeError::invalid(failed.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Parses;

    fn args(text: &str) -> Map<String, Value> {
        serde_json::from_str(text).expect("an object")
    }

    #[test]
    fn an_act_round_trips_and_names_itself() {
        let act = Act::Add {
            todo: "permitfiling".to_owned(),
            fields: TodoFields {
                body: "File the permit renewal".to_owned(),
                rationale: "A real consequence within weeks.".to_owned(),
                kind: "todo".to_owned(),
                category: String::new(),
                price: Price::Flat { percent: 40 },
                detail: String::new(),
                waiting_on: String::new(),
            },
            from: Provenance::default(),
        };
        let printed = serde_json::to_string(&act).expect("an act serialises");
        assert!(printed.contains(r#""act":"add""#), "{printed}");
        assert!(printed.contains(r#""how":"flat""#), "{printed}");
        let read: Act = serde_json::from_str(&printed).expect("and reads back");
        assert_eq!(read, act);
    }

    #[test]
    fn a_changeset_with_no_acts_is_refused_everywhere() {
        assert_eq!(Changeset::new("life", Vec::new()).unwrap_err(), NoActs);
        assert!(serde_json::from_str::<Changeset>(r#"{"chain":"life","acts":[]}"#).is_err());
        let one: Changeset = serde_json::from_str(
            r#"{"chain":"life","acts":[{"act":"complete","todo":"x","note":""}]}"#,
        )
        .expect("one act");
        assert_eq!(one.chain(), "life");
        assert_eq!(one.acts().len(), 1);
    }

    /// Reads, stores, builds and draws a `Recur` reprice.
    #[test]
    fn a_reprice_can_make_a_chore_recur() {
        let listed = acts_of(
            args(
                r#"{"acts": [{"act": "reprice", "todo": "brush", "recur_every_days": 60,
                             "cycle_from_priority": "100%", "cycle_to_priority": "30%",
                             "rationale": "Vinegar it every two months."}]}"#,
            )
            .get("acts"),
        )
        .expect("one act");
        let cycle = crate::todo::Cycle {
            days: 60,
            from_percent: 100,
            to_percent: 30,
        };
        assert!(
            matches!(&listed[0], Act::Reprice { priority: Price::Recur(read), .. } if *read == cycle),
            "{listed:?}"
        );
        let printed = serde_json::to_string(&listed[0]).expect("an act serialises");
        assert!(
            printed.contains(r#""how":"recur","days":60,"from_percent":100,"to_percent":30"#),
            "{printed}"
        );
        assert_eq!(
            serde_json::from_str::<Act>(&printed).expect("and reads back"),
            listed[0]
        );

        let at = Datetime::new(2026, 9, 26, 12, 0, 0, 0).expect("an instant");
        let brush = authoring::build_todo(
            &NewTodo {
                todo: "brush".to_owned(),
                fields: TodoFields {
                    body: "Vinegar the toothbrush".to_owned(),
                    rationale: "A habit.".to_owned(),
                    kind: "todo".to_owned(),
                    category: String::new(),
                    price: Price::Flat { percent: 80 },
                    detail: String::new(),
                    waiting_on: String::new(),
                },
                source: None,
                subtodos: Vec::new(),
            },
            crate::zone::Moment {
                at,
                zone: crate::zone::Zone::UTC,
            },
            "ada",
            &[],
            &Parses,
        )
        .expect("a todo");
        let untrusted = prodrome_core::policy::Untrusted::none();
        let building = Building {
            now: crate::zone::Moment {
                at,
                zone: crate::zone::Zone::UTC,
            },
            by: Writer::Assistant,
            categories: &[],
            untrusted: &untrusted,
            typeset: &Parses,
        };
        let (rows, problem) = shown(
            &Changeset::new("life", listed.clone()).expect("one act"),
            &Preview::chain(std::slice::from_ref(&brush)),
            &building,
        );
        assert_eq!(problem, "");
        assert_eq!(rows[0].act.word(), "repriced");
        let priced = rows[0]
            .act
            .changes()
            .iter()
            .find(|change| change.field == "priority")
            .expect("the price moved");
        assert_eq!(priced.before, "80%");
        assert_eq!(
            priced.after,
            "80%; every 60 days after a pass, from 100% to 30%"
        );

        let built = listed[0]
            .events(Preview::chain(&[brush]).nodes(), &building)
            .expect("a reprice");
        let [TodoEvent::Authored(built)] = built.as_slice() else {
            panic!("a reprice is an Authored");
        };
        let spec = &built.payload.spec;
        assert!(
            matches!(spec.out(), prodrome_core::term::TermF::Recur { todo, .. } if todo == "brush"),
            "{}",
            prodrome_core::fpl::print_term(spec)
        );
    }
}
