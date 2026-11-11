//! The chain projected onto the wire: conversion only, every number from `prodrome_core`.
//!
//! `rich` travels beside the source, never instead, and is `null` when uncompiled or untrusted.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::record::{Authored, Entry, Envelope, Node, TodoEvent};
use prodrome_core::event::{Hash, parents_of, seal_hash};
use prodrome_core::fpl::{self, Env, LinkError};
use prodrome_core::literal::{Value, print_literal};
use prodrome_core::payload::Payload as _;
use prodrome_core::policy::Untrusted;
use prodrome_core::term::{Term, TermF};
use regex::Regex;

use crate::believed::one_outcome;
use crate::render::Render;
use crate::wire::todo as wire;
use suzatary_base::capability::Datetime;
use suzatary_base::escape::{Escaped, Quoted, escape_markup};
use suzatary_base::tree::{self, Classifier, Member, Members, Place, Segment, Tree, Trie};

/// Whether a row's content is still its untrusted writer's: authored by an actor the policy does
/// not confirm and never re-authored by one it does.
#[must_use]
pub fn provisional_content(entry: &Entry) -> bool {
    matches!(
        entry.confidence,
        prodrome_core::view::Confidence::Provisional(
            prodrome_core::view::Provisional::Content
                | prodrome_core::view::Provisional::ClaimedAndContent
        )
    )
}

/// One stored object on the wire: an event with its todo, writer and instant, or an envelope that
/// claims none of them.
#[must_use]
pub fn link(name: &Hash, envelope: &Envelope) -> wire::Link {
    wire::Link {
        hash: name.as_str().to_owned(),
        parents: parents_of(envelope)
            .iter()
            .map(|p| p.as_str().to_owned())
            .collect(),
        literal: prodrome_core::event::canonical_envelope(envelope),
        holds: match envelope.event() {
            None => wire::Holds::Envelope {
                kind: envelope_kind(envelope).to_owned(),
            },
            Some(event) => wire::Holds::Event {
                kind: event.kind_name().to_owned(),
                todo: event.todo().as_str().to_owned(),
                actor: event.actor().as_str().to_owned(),
                at: fpl::iso(fpl::instant_of(event.at())),
                note: note_of(event),
                reverses: wire::Reverses::from(crate::authoring::Reversal::of_note(&note_of(
                    event,
                ))),
            },
        },
    }
}

/// An event's own `note` field.
fn note_of(event: &TodoEvent) -> String {
    match event {
        TodoEvent::Created(e) => e.note.clone(),
        TodoEvent::Completed(e)
        | TodoEvent::Cancelled(e)
        | TodoEvent::Reopened(e)
        | TodoEvent::Tended(e) => e.note.clone(),
        TodoEvent::SpecRevised(e) => e.note.clone(),
        TodoEvent::Authored(e) => e.payload.note.clone(),
    }
}

fn envelope_kind(envelope: &Envelope) -> &'static str {
    envelope.name()
}

/// The events of these nodes, in their order, without the merges that carry none.
#[must_use]
pub fn events_of(nodes: &[Node]) -> Vec<TodoEvent> {
    nodes
        .iter()
        .filter_map(|node| node.event().cloned())
        .collect()
}

/// One todo's own events beside their hashes, as undo and redo read them: causal, and among
/// writes nothing orders by the instant stamped, then the name.
///
/// # Panics
/// Never: ancestry is acyclic, so some write always waits on nothing.
#[must_use]
pub fn stream_of(nodes: &[Node], todo: &str) -> Vec<(Hash, TodoEvent)> {
    let parents: BTreeMap<&Hash, &[Hash]> = nodes
        .iter()
        .map(|node| (node.name(), node.parents()))
        .collect();
    let mine: Vec<(&Hash, &TodoEvent)> = nodes
        .iter()
        .filter_map(|node| {
            let event = node.event()?;
            (event.todo().as_str() == todo).then_some((node.name(), event))
        })
        .collect();
    let below = |name: &Hash| -> BTreeSet<&Hash> {
        let mut seen = BTreeSet::new();
        let mut pending: Vec<&Hash> = parents.get(name).into_iter().copied().flatten().collect();
        while let Some(next) = pending.pop() {
            if seen.insert(next) {
                pending.extend(parents.get(next).into_iter().copied().flatten());
            }
        }
        seen
    };
    let mut waiting: Vec<(BTreeSet<&Hash>, &Hash, &TodoEvent)> = mine
        .iter()
        .map(|(name, event)| {
            let before: BTreeSet<&Hash> = below(name)
                .into_iter()
                .filter(|earlier| mine.iter().any(|(held, _)| held == earlier))
                .collect();
            (before, *name, *event)
        })
        .collect();
    let mut out = Vec::with_capacity(waiting.len());
    while !waiting.is_empty() {
        let next = waiting
            .iter()
            .enumerate()
            .filter(|(_, (before, _, _))| before.is_empty())
            .min_by_key(|(_, (_, name, event))| (event.at(), *name))
            .map(|(at, _)| at)
            .expect("ancestry is acyclic");
        let (_, name, event) = waiting.remove(next);
        for (before, _, _) in &mut waiting {
            before.remove(name);
        }
        out.push((name.clone(), event.clone()));
    }
    out
}

/// Rendered means confirmed: an untrusted actor's content is served as source.
#[must_use]
pub fn renders(record: &Authored, untrusted: &Untrusted) -> bool {
    !untrusted.actors().contains(&record.actor)
}

/// One field's text as the typst markup it renders, `None` when it is empty: the body and the
/// detail as stored, the waiting-on and the rationale escaped as data.
#[must_use]
pub(crate) fn rich_source(field: &str, text: &str) -> Option<Escaped> {
    if text.is_empty() {
        return None;
    }
    match field {
        "body" | "detail" => Some(Escaped::from_chain(text.to_owned())),
        "waiting_on" => Some(escape_markup(&string_text(text))),
        "rationale" => Some(escape_markup(text)),
        _ => None,
    }
}

/// Each present field as the typst markup it stands for (`rich_source`).
#[must_use]
pub fn rich_sources(record: &Authored) -> Vec<(&'static str, Escaped)> {
    let rationale = record.payload.rationale.join(" ");
    [
        ("body", record.payload.body.as_str()),
        ("detail", record.payload.detail.as_str()),
        ("waiting_on", record.payload.waiting_on.as_str()),
        ("rationale", rationale.as_str()),
    ]
    .into_iter()
    .filter_map(|(field, text)| rich_source(field, text).map(|source| (field, source)))
    .collect()
}

/// Typst string source as the prose it stands for, undoing only `\\` and `\"`.
///
/// Lenient where [`suzatary_base::escape::unescape_string`] is strict, since stored records predate it.
#[must_use]
pub fn string_text(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && matches!(chars.peek(), Some('\\' | '"')) {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Every trusted markup fragment worth compiling, for a warm.
#[must_use]
pub fn renderable<'a>(
    records: impl IntoIterator<Item = &'a Authored>,
    untrusted: &Untrusted,
) -> Vec<Escaped> {
    records
        .into_iter()
        .filter(|record| renders(record, untrusted))
        .flat_map(|record| rich_sources(record).into_iter().map(|(_, source)| source))
        .collect()
}

/// The four fields as typst's HTML, `None` for any absent, uncompiled or untrusted one.
fn rich<R: Render + ?Sized>(render: &R, record: &Authored, untrusted: &Untrusted) -> wire::Rich {
    if !renders(record, untrusted) {
        return wire::Rich {
            body: None,
            detail: None,
            rationale: None,
            waiting_on: None,
            keys: wire::RichKeys {
                body: None,
                detail: None,
                rationale: None,
                waiting_on: None,
            },
        };
    }
    let sources = rich_sources(record);
    let source = |name: &str| {
        sources
            .iter()
            .find(|(field, _)| *field == name)
            .map(|(_, source)| source)
    };
    let found = |name: &str| {
        source(name)
            .and_then(|source| render.cached(source))
            .map(|html| html.as_str().to_owned())
    };
    let key = |name: &str| {
        source(name).and_then(|source| {
            crate::rendering::MARKUP
                .key(&crate::rendering::Markup {
                    source: source.clone(),
                })
                .ok()
        })
    };
    wire::Rich {
        body: found("body"),
        detail: found("detail"),
        rationale: found("rationale"),
        waiting_on: found("waiting_on"),
        keys: wire::RichKeys {
            body: key("body"),
            detail: key("detail"),
            rationale: key("rationale"),
            waiting_on: key("waiting_on"),
        },
    }
}

/// A content record on the wire.
pub fn authored<R: Render + ?Sized>(
    render: &R,
    record: &Authored,
    untrusted: &Untrusted,
) -> wire::Authored {
    wire::Authored {
        rich: rich(render, record, untrusted),
        block: wire::Block::of(record.payload.kind.as_str()),
        created: fpl::instant_of(record.payload.created).date().to_string(),
        body: record.payload.body.as_str().to_owned(),
        text: suzatary_base::escape::markup_text(record.payload.body.as_str()),
        detail: record.payload.detail.as_str().to_owned(),
        category: record
            .payload
            .category
            .as_ref()
            .map_or_else(String::new, |name| name.as_str().to_owned()),
        waiting_on: record.payload.waiting_on.as_str().to_owned(),
        rationale: record.payload.rationale.clone(),
        spec: record.payload.spec().map(suzatary_base::term_json::to_json),
        actor: record.actor.as_str().to_owned(),
        at: fpl::iso(fpl::instant_of(record.at)),
        source: record
            .payload
            .source
            .as_ref()
            .map(|source| wire::AuthoredSource {
                sender: source.sender.as_str().to_owned(),
                subject: source.subject.as_str().to_owned(),
                date: fpl::instant_of(source.date).date().to_string(),
                hash: source.hash.as_str().to_owned(),
                thread_id: source.thread_id.as_str().to_owned(),
                message_id: source.message_id.as_str().to_owned(),
            }),
        subtodos: record
            .payload
            .subtodos
            .iter()
            .map(|sub| crate::wire::record::SubTodo {
                body: sub.body.as_str().to_owned(),
                done: sub.done,
            })
            .collect(),
    }
}

/// Whether the piece of `term` in force at `now` holds a `Recur` over `todo`'s own tendings.
#[must_use]
pub fn recurs(term: &Term, todo: &str, now: fpl::Instant) -> bool {
    match term.out() {
        TermF::Recur { todo: tended, .. } if tended == todo => true,
        TermF::Piecewise(schedule) => recurs(schedule.at(now), todo, now),
        node => node
            .children()
            .into_iter()
            .any(|child| recurs(child, todo, now)),
    }
}

/// One todo as the folds see it, its content looked up by name among the caller's objects.
pub fn entry<R: Render + ?Sized>(
    render: &R,
    entry: &Entry,
    by_hash: &BTreeMap<Hash, &Envelope>,
    untrusted: &Untrusted,
    now: Datetime,
) -> wire::Entry {
    wire::Entry {
        todo: entry.key.as_str().to_owned(),
        state: wire::EntryState::from(one_outcome(entry.outcome())),
        states: entry
            .outcome()
            .iter()
            .map(|outcome| wire::EntryState::from(*outcome))
            .collect(),
        at: entry.at(),
        claimed: wire::ClaimedState::from(
            entry
                .claim
                .as_ref()
                .and_then(|claim| one_outcome(&claim.outcome)),
        ),
        value: wire::Fulfillment::of(entry.value()),
        recurring: recurs(entry.spec(), entry.key.as_str(), fpl::instant_of(now)),
        unconfirmed: entry.confidence.is_provisional(),
        conflicts: entry
            .conflicts
            .iter()
            .map(|(kind, names)| {
                (
                    kind.as_str().to_owned(),
                    names.iter().map(|n| n.as_str().to_owned()).collect(),
                )
            })
            .collect(),
        authored: content_of(entry, by_hash).map(|record| authored(render, record, untrusted)),
        candidates: if entry.content().len() > 1 {
            candidates_of(entry, by_hash)
                .into_iter()
                .map(|record| authored(render, record, untrusted))
                .collect()
        } else {
            Vec::new()
        },
        stream: entry
            .stream
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect(),
    }
}

/// The content record a single-record reading shows: the entry's first candidate (see
/// [`crate::believed::Believed`]), out of the objects the caller holds.
#[must_use]
pub fn content_of<'a>(
    entry: &Entry,
    by_hash: &'a BTreeMap<Hash, &'a Envelope>,
) -> Option<&'a Authored> {
    candidates_of(entry, by_hash).into_iter().next()
}

/// Every candidate content record an entry names, by name.
#[must_use]
pub(crate) fn candidates_of<'a>(
    entry: &Entry,
    by_hash: &'a BTreeMap<Hash, &'a Envelope>,
) -> Vec<&'a Authored> {
    entry
        .content()
        .iter()
        .filter_map(|name| match by_hash.get(name)?.event()? {
            TodoEvent::Authored(record) => Some(record.as_ref()),
            _ => None,
        })
        .collect()
}

/// A todo as a neighbour reads it.
#[must_use]
pub fn neighbour(entry: &Entry) -> wire::Neighbour {
    wire::Neighbour {
        todo: entry.key.as_str().to_owned(),
        state: wire::EntryState::from(one_outcome(entry.outcome())),
        value: wire::Fulfillment::of(entry.value()),
        unconfirmed: entry.confidence.is_provisional(),
    }
}

/// The one explicit way a todo names another.
static TODO_LINK: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"#link\("todo:([A-Za-z0-9_-]+)"\)"#).expect("a literal pattern")
});

/// Ids shorter than this are not matched as mentions; they are words.
const MIN_MENTION: usize = 8;

/// Which todos each todo refers to, by `#link("todo:<id>")` or by spelling another's id: one
/// alternation, longest first, scanned once per record.
///
/// # Errors
/// The alternation past the regex engine's size limit.
pub fn references(
    entries: &[Entry],
    by_hash: &BTreeMap<Hash, &Envelope>,
) -> Result<BTreeMap<String, Vec<String>>, regex::Error> {
    let ids: BTreeSet<&str> = entries.iter().map(|e| e.key.as_str()).collect();
    let mut long: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| id.len() >= MIN_MENTION)
        .collect();
    long.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    let mention = if long.is_empty() {
        None
    } else {
        let alternation = long
            .iter()
            .map(|id| regex::escape(id))
            .collect::<Vec<_>>()
            .join("|");
        Some(Regex::new(&format!(
            r"(?:^|[^A-Za-z0-9_])({alternation})(?:$|[^A-Za-z0-9_])"
        ))?)
    };
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in entries {
        let Some(record) = content_of(entry, by_hash) else {
            continue;
        };
        let text = [
            record.payload.body.as_str(),
            record.payload.detail.as_str(),
            record.payload.waiting_on.as_str(),
        ]
        .into_iter()
        .map(str::to_owned)
        .chain(record.payload.rationale.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n");
        let mine = entry.key.as_str();
        let mut found: Vec<String> = Vec::new();
        for caught in TODO_LINK.captures_iter(&text) {
            let named = &caught[1];
            if ids.contains(named) && named != mine && !found.iter().any(|f| f == named) {
                found.push(named.to_owned());
            }
        }
        if let Some(pattern) = mention.as_ref() {
            // No lookaround, so the scan resumes just past each id to catch shared separators.
            let mut from = 0usize;
            while let Some(id) = pattern
                .captures_at(&text, from)
                .and_then(|caught| caught.get(1))
            {
                let other = id.as_str();
                if other != mine && !found.iter().any(|f| f == other) {
                    found.push(other.to_owned());
                }
                from = next_boundary(&text, id.end());
            }
        }
        if !found.is_empty() {
            out.insert(mine.to_owned(), found);
        }
    }
    Ok(out)
}

/// The next character boundary at or after `at`, so slicing never panics.
fn next_boundary(text: &str, at: usize) -> usize {
    let mut index = at.min(text.len());
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// The inverse of [`references`].
#[must_use]
pub fn backlinks(refs: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (source, targets) in refs {
        for target in targets {
            out.entry(target.clone()).or_default().push(source.clone());
        }
    }
    for names in out.values_mut() {
        names.sort();
    }
    out
}

/// Fields hidden on a first record when they are empty.
const HIDDEN_WHEN_EMPTY: [&str; 8] = [
    "note",
    "notes",
    "subtodos",
    "rationale",
    "source",
    "detail",
    "waiting_on",
    "category",
];

/// Each content field as its canonical print, skipping the event's identity.
#[must_use]
pub fn prints(record: &Authored) -> Vec<(String, String)> {
    let Value::Call(call) = record.to_value() else {
        return Vec::new();
    };
    call.fields
        .into_iter()
        .filter(|(name, _)| !matches!(name.as_str(), "todo" | "at" | "actor"))
        .map(|(name, value)| (name, print_literal(&value)))
        .collect()
}

/// What one `Authored` changed against the one before; a first hides the fields it left absent.
#[must_use]
pub(crate) fn authored_diff(
    previous: Option<&Authored>,
    current: &Authored,
) -> Vec<wire::FieldChange> {
    let before: BTreeMap<String, String> = previous
        .map(|p| prints(p).into_iter().collect())
        .unwrap_or_default();
    prints(current)
        .into_iter()
        .filter_map(|(name, after)| {
            let was = before.get(&name).cloned().unwrap_or_default();
            if was == after {
                return None;
            }
            if previous.is_none()
                && HIDDEN_WHEN_EMPTY.contains(&name.as_str())
                && matches!(after.as_str(), "''" | "()" | "None")
            {
                return None;
            }
            Some(wire::FieldChange {
                field: name,
                before: was,
                after,
            })
        })
        .collect()
}

/// Each `Authored` in a stream against the one before it, keyed by its own hash.
#[must_use]
pub fn diffs(
    stream: &[Hash],
    by_hash: &BTreeMap<Hash, &Envelope>,
) -> BTreeMap<String, Vec<wire::FieldChange>> {
    let mut out = BTreeMap::new();
    let mut previous: Option<&Authored> = None;
    for name in stream {
        let Some(TodoEvent::Authored(record)) = by_hash.get(name).and_then(|e| e.event()) else {
            continue;
        };
        out.insert(name.as_str().to_owned(), authored_diff(previous, record));
        previous = Some(record);
    }
    out
}

/// The core's priced explanation tree at a moment, `None` when the function does not link.
#[must_use]
pub fn explain(entry: &Entry, now: Datetime, env: &Env) -> Option<fpl::Explanation> {
    let spec = entry.linked()?;
    Some(fpl::explained(spec, fpl::instant_of(now), env))
}

/// One node of the list's tree: where it sits, its term, and the core's value of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The group or todo.
    pub place: Place,
    /// Its term: `Ref(todo)`, or the `Conj` of its children.
    pub term: Term,
    /// `fulfillment(link(term))`: a number, `∅` (`None`), or why it does not link.
    pub worth: Result<Option<f64>, LinkError>,
}

/// The todos the list shows (open, with content, not a decision) as the classifiers read them.
#[must_use]
pub fn members(entries: &[Entry], by_hash: &BTreeMap<Hash, &Envelope>) -> Members {
    let listed: Vec<(&Entry, &Authored)> = entries
        .iter()
        .filter(|entry| one_outcome(entry.outcome()).is_none())
        .filter_map(|entry| Some((entry, content_of(entry, by_hash)?)))
        .filter(|(_, record)| !record.payload.is_decision())
        .collect();
    let mut parents = tree::parents(
        listed
            .iter()
            .map(|(entry, record)| (&entry.key, record.payload.spec())),
    );
    listed
        .into_iter()
        .map(|(entry, record)| {
            let member = Member {
                title: suzatary_base::escape::markup_text(record.payload.body.as_str()),
                category: record.payload.category.as_ref().map_or_else(
                    || crate::decoration::kind_category(record.payload.kind.as_str()).to_owned(),
                    |name| name.as_str().to_owned(),
                ),
                worth: entry.price.value.clone(),
                parents: parents.remove(&entry.key).unwrap_or_default(),
            };
            (entry.key.clone(), member)
        })
        .collect()
}

/// The list as one tree of terms, each priced by the core, so nothing here averages.
#[must_use]
pub fn tree(
    entries: &[Entry],
    members: &Members,
    classifier: &Classifier,
    now: Datetime,
    env: &fpl::Env,
) -> Tree<Row> {
    let specs: BTreeMap<String, Term> = entries
        .iter()
        .map(|entry| (entry.key.as_str().to_owned(), entry.spec().clone()))
        .collect();
    let now = fpl::instant_of(now);
    Trie::of(members, classifier)
        .tree()
        .scan(&mut tree::term)
        .map(&mut |(place, term)| Row {
            worth: fpl::link(&term, &specs).map(|closed| fpl::fulfillment(&closed, now, env)),
            place,
            term,
        })
        .cata(&mut |row, mut children: Vec<Tree<Row>>| {
            children.sort_by(|a, b| urgency(&a.node.worth, &b.node.worth));
            Tree {
                node: row,
                children,
            }
        })
}

/// The tree on the wire: each node's place, its worth and its term's print.
#[must_use]
pub fn tree_node(tree: Tree<Row>, members: &Members) -> wire::TreeNode {
    use wire::{GroupBy, TreeNode};
    tree.cata(&mut |row: Row, children| {
        let (worth, term) = (worth(row.worth), fpl::print_term(&row.term));
        match row.place {
            Place::All => TreeNode::Group {
                by: GroupBy::All,
                label: String::new(),
                worth,
                term,
                children,
            },
            Place::At(Segment::Category(label)) => TreeNode::Group {
                by: GroupBy::Category,
                label,
                worth,
                term,
                children,
            },
            Place::At(Segment::Band(band)) => TreeNode::Group {
                by: GroupBy::Urgency,
                label: band.label().to_owned(),
                worth,
                term,
                children,
            },
            Place::At(Segment::Todo(todo)) => {
                let member = &members[&todo];
                TreeNode::Todo {
                    todo: todo.as_str().to_owned(),
                    title: member.title.clone(),
                    category: member.category.clone(),
                    worth,
                    term,
                    children,
                }
            }
        }
    })
}

fn worth(worth: Result<Option<f64>, LinkError>) -> wire::Worth {
    match worth {
        Ok(Some(value)) => wire::Worth::Value(value),
        Ok(None) => wire::Worth::Absent,
        Err(LinkError::Unknown(todo)) => wire::Worth::Unknown(todo),
        Err(LinkError::Cycle(path)) => wire::Worth::Cycle(path),
    }
}

/// Sibling order: unlinkable first, then ascending in value, then `∅`.
fn urgency(
    left: &Result<Option<f64>, LinkError>,
    right: &Result<Option<f64>, LinkError>,
) -> Ordering {
    let rank = |worth: &Result<Option<f64>, LinkError>| match worth {
        Err(_) => (0, 0.0),
        Ok(Some(value)) => (1, *value),
        Ok(None) => (2, 0.0),
    };
    let (left, right) = (rank(left), rank(right));
    left.0
        .cmp(&right.0)
        .then_with(|| left.1.total_cmp(&right.1))
}

/// Every object by name, for the lookups above.
#[must_use]
pub fn by_hash(objects: &[(Hash, Envelope)]) -> BTreeMap<Hash, &Envelope> {
    objects
        .iter()
        .map(|(name, envelope)| (name.clone(), envelope))
        .collect()
}

/// Every object's own name, recomputed.
#[must_use]
pub fn named(objects: &[Envelope]) -> Vec<(Hash, &Envelope)> {
    objects
        .iter()
        .map(|envelope| (seal_hash(envelope), envelope))
        .collect()
}

/// The `Quoted` a stored `waiting_on` stands for.
#[must_use]
pub fn quoted(source: &str) -> Quoted {
    Quoted::from_chain(source.to_owned())
}

#[cfg(test)]
mod tests {
    use crate::record::{MarkupSource, mk_authored, mk_source};

    #[test]
    fn a_data_field_renders_as_escaped_markup_and_a_markup_field_as_written() {
        use super::{escape_markup, rich_source};
        let text = "the *notary* #2";
        let source = |field| rich_source(field, text).map(|s| s.as_str().to_owned());
        let escaped = Some(escape_markup(text).as_str().to_owned());
        assert_eq!(source("waiting_on"), escaped);
        assert_eq!(source("rationale"), escaped);
        assert_ne!(source("waiting_on"), Some(text.to_owned()));
        assert_eq!(source("body"), Some(text.to_owned()));
        assert!(rich_source("body", "").is_none());
    }
    use prodrome_core::event::{Actor, mk_completed, mk_reopened, mk_sealed};
    use prodrome_core::literal::Datetime as Stamp;

    use super::*;
    use crate::render::Html;
    use suzatary_base::capability::RenderError;

    /// A render that echoes the markup, so a test sees which fragment reached it.
    struct Echo;
    impl suzatary_base::capability::Typeset for Echo {
        fn closed(&self, source: &str) -> Result<String, suzatary_base::capability::Rejected> {
            suzatary_base::capability::parsed(source)
                .map(|()| "<!DOCTYPE html><html><body></body></html>".to_owned())
        }
    }
    impl Render for Echo {
        fn html(&self, markup: &Escaped) -> Result<Html, RenderError> {
            Ok(Html::from_render(format!("<p>{}</p>", markup.as_str())))
        }
        fn cached(&self, markup: &Escaped) -> Option<Html> {
            Some(Html::from_render(format!("<p>{}</p>", markup.as_str())))
        }
        fn warm(&self, markup: &[Escaped]) -> usize {
            markup.len()
        }
        fn page(&self, page: &crate::template::Called) -> Result<Html, RenderError> {
            Ok(Html::from_render(page.job().state.clone()))
        }
    }

    fn at(day: u32) -> Stamp {
        Stamp::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    fn untrusted() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    fn record(todo: &str, actor: &str) -> Authored {
        let TodoEvent::Authored(record) = mk_authored(
            todo,
            at(1),
            actor,
            "todo",
            at(1),
            MarkupSource::from_chain("File the \\*permit\\(s\\)\\* renewal"),
            fpl::mk_flat(0.4).expect("a spec"),
            vec!["Missing it is not recoverable.".to_owned()],
            "work",
            "\\\"the lawyer\\\"",
            MarkupSource::empty(),
            Some(mk_source(
                "counsel",
                "Your deed",
                at(1),
                "abc123def",
                "",
                "",
            )),
            vec![],
            vec![],
            "",
        )
        .expect("a record") else {
            unreachable!("mk_authored answers an Authored")
        };
        *record
    }

    /// Two writes over one record, neither over the other: the wire carries both, and a completion
    /// beside a reopening is two states.
    #[test]
    fn a_conflict_reaches_the_wire_as_every_candidate() {
        let first = TodoEvent::Authored(Box::new(record("permit", "ada")));
        let root = mk_sealed(None, first);
        let base = seal_hash(&root);
        let mut changed = record("permit", "ada");
        changed.payload.body = MarkupSource::from_chain("File the permit today");
        changed.at = at(2);
        let other = mk_sealed(Some(base.clone()), TodoEvent::Authored(Box::new(changed)));
        let mut again = record("permit", "ada");
        again.payload.body = MarkupSource::from_chain("File the permit tomorrow");
        again.at = at(2);
        let another = mk_sealed(Some(base.clone()), TodoEvent::Authored(Box::new(again)));
        let done = mk_sealed(
            Some(base),
            mk_completed("permit", at(3), "ada", "").expect("an event"),
        );
        let objects: Vec<(Hash, Envelope)> = [root, other, another, done]
            .into_iter()
            .map(|object| (seal_hash(&object), object))
            .collect();
        let nodes = prodrome_core::dag::Dag::from_iter(objects.clone())
            .nodes()
            .expect("a closed set reads");
        let entries = prodrome_core::view::entries(&nodes, at(4), &untrusted()).expect("folds");
        let shown = entry(&Echo, &entries[0], &by_hash(&objects), &untrusted(), at(4));
        let bodies: BTreeSet<String> = shown
            .candidates
            .iter()
            .map(|candidate| candidate.body.clone())
            .collect();
        assert_eq!(bodies.len(), 2, "{bodies:?}");
        assert!(
            bodies
                .iter()
                .all(|body| body.starts_with("File the permit t"))
        );
        assert_eq!(
            shown.authored.map(|record| record.body),
            shown.candidates.first().map(|c| c.body.clone())
        );
        assert_eq!(shown.states, vec![wire::EntryState::Completed]);
        assert_eq!(shown.state, wire::EntryState::Completed);
    }

    /// A completion a tab stamped in its device's zone before this series can read hours
    /// before the todo it completes was authored, though it rests on it. Every reading answers
    /// and none panics: before the act's stamp the todo reads as the chain without the act does,
    /// from the stamp it is done, unpriced until its record is believed and 100% after.
    #[test]
    fn an_act_stamped_before_its_todos_first_event_is_read_without_misfiling() {
        let root = mk_sealed(None, TodoEvent::Authored(Box::new(record("permit", "ada"))));
        let early = Stamp::new(2026, 9, 1, 8, 2, 0, 0).expect("an instant");
        let done = mk_sealed(
            Some(seal_hash(&root)),
            mk_completed("permit", early, "ada", "").expect("an event"),
        );
        let read = |objects: &[(Hash, Envelope)], t: Stamp| {
            let nodes = prodrome_core::dag::Dag::from_iter(objects.to_vec())
                .nodes()
                .expect("a closed set reads");
            prodrome_core::view::entries(&nodes, t, &untrusted())
                .expect("folds")
                .iter()
                .map(|read| entry(&Echo, read, &by_hash(objects), &untrusted(), t))
                .map(|shown| (shown.state, shown.value.and_then(wire::Fulfillment::number)))
                .collect::<Vec<_>>()
        };
        let alone: Vec<(Hash, Envelope)> = vec![(seal_hash(&root), root.clone())];
        let both: Vec<(Hash, Envelope)> = [root, done]
            .into_iter()
            .map(|object| (seal_hash(&object), object))
            .collect();
        let before = Stamp::new(2026, 9, 1, 7, 0, 0, 0).expect("an instant");
        assert_eq!(
            read(&both, before),
            read(&alone, before),
            "nothing moves before the stamp"
        );
        for (hour, minute, value) in [
            (8, 2, None),
            (10, 0, None),
            (12, 0, Some(1.0)),
            (23, 59, Some(1.0)),
        ] {
            let t = Stamp::new(2026, 9, 1, hour, minute, 0, 0).expect("an instant");
            assert_eq!(
                read(&both, t),
                vec![(wire::EntryState::Completed, value)],
                "at {hour}:{minute:02}"
            );
        }
    }

    #[test]
    fn an_untrusted_records_rich_is_four_nulls_and_its_source_is_whole() {
        let theirs = record("email-harrow", "triage");
        let mine = record("file-permit", "ada");
        let policy = untrusted();
        assert!(!renders(&theirs, &policy));
        assert!(renders(&mine, &policy));

        let hidden = authored(&Echo, &theirs, &policy);
        assert_eq!(hidden.rich.body, None);
        assert_eq!(hidden.rich.detail, None);
        assert_eq!(hidden.rich.rationale, None);
        assert_eq!(hidden.rich.waiting_on, None);
        assert_eq!(hidden.body, "File the \\*permit\\(s\\)\\* renewal");

        let shown = authored(&Echo, &mine, &policy);
        assert_eq!(
            shown.rich.body.as_deref(),
            Some("<p>File the \\*permit\\(s\\)\\* renewal</p>")
        );
    }

    #[test]
    fn an_untrusted_records_text_is_its_words_without_the_markup() {
        let mut theirs = record("email-harrow", "triage");
        theirs.payload.body = MarkupSource::from_chain(
            r#"Email *Harrow* about #link("https://example.org")[the deed]"#,
        );
        let hidden = authored(&Echo, &theirs, &untrusted());
        assert_eq!(hidden.rich.body, None);
        assert_eq!(hidden.text, "Email Harrow about the deed");
        assert_eq!(
            hidden.body,
            r#"Email *Harrow* about #link("https://example.org")[the deed]"#
        );
    }

    #[test]
    fn waiting_on_and_rationale_are_converted_on_the_way_to_typst() {
        let mine = record("file-permit", "ada");
        let sources = rich_sources(&mine);
        let waiting = sources
            .iter()
            .find(|(name, _)| *name == "waiting_on")
            .expect("a waiting-on")
            .1
            .clone();
        // `"` is not structural in markup, so the quotes stand unescaped.
        assert_eq!(waiting.as_str(), "\"the lawyer\"");
        let rationale = sources
            .iter()
            .find(|(name, _)| *name == "rationale")
            .expect("a rationale")
            .1
            .clone();
        assert_eq!(rationale.as_str(), "Missing it is not recoverable.");
    }

    #[test]
    fn a_lenient_string_text_undoes_only_the_two_escapes_a_string_has() {
        assert_eq!(string_text(r#"\"quoted\""#), "\"quoted\"");
        assert_eq!(string_text(r"a\\b"), r"a\b");
        assert_eq!(string_text("her@example.test"), "her@example.test");
        // Typst strings have no `\n`, so these two characters stay two characters.
        assert_eq!(string_text(r"a\nb"), r"a\nb");
    }

    #[test]
    fn a_merge_carries_no_actor_no_todo_and_no_instant() {
        let event = mk_completed("alpha", at(2), "ada", "").expect("valid");
        let sealed = mk_sealed(None, event);
        let named = seal_hash(&sealed);
        let wired = link(&named, &sealed);
        assert_eq!(
            wired.holds,
            wire::Holds::Event {
                kind: "Completed".to_owned(),
                todo: "alpha".to_owned(),
                actor: "ada".to_owned(),
                at: "2026-09-02T12:00:00".to_owned(),
                note: String::new(),
                reverses: wire::Reverses::Nothing,
            }
        );
        assert_eq!(wired.parents, Vec::<String>::new());
        assert_eq!(wired.hash, named.as_str());

        let merge = prodrome_core::event::mk_woven(
            vec![named.clone(), Hash::new("b".repeat(64)).expect("hex")],
            None,
        )
        .expect("two parents");
        let joined = link(&seal_hash(&merge), &merge);
        assert_eq!(
            joined.holds,
            wire::Holds::Envelope {
                kind: "Woven".to_owned()
            }
        );
        assert_eq!(joined.parents.len(), 2);
    }

    /// What an event's link reverses; an envelope reverses nothing, being no event.
    fn reverses(link: &wire::Link) -> Option<&wire::Reverses> {
        match &link.holds {
            wire::Holds::Event { reverses, .. } => Some(reverses),
            wire::Holds::Envelope { .. } => None,
        }
    }

    /// A link says what its event reverses as a value read from the note, and an ordinary
    /// event's link reverses nothing.
    #[test]
    fn a_link_names_what_its_undo_or_redo_reverses() {
        let target = Hash::new("c".repeat(64)).expect("hex");
        let note = crate::authoring::Reversal::Undo { of: target.clone() }.note();
        let undo = mk_sealed(
            None,
            mk_reopened("alpha", at(3), "ada", &note).expect("valid"),
        );
        assert_eq!(
            reverses(&link(&seal_hash(&undo), &undo)),
            Some(&wire::Reverses::Undo {
                of: target.as_str().to_owned()
            })
        );
        let plain = mk_sealed(
            None,
            mk_completed("alpha", at(2), "ada", "moved").expect("valid"),
        );
        assert_eq!(
            reverses(&link(&seal_hash(&plain), &plain)),
            Some(&wire::Reverses::Nothing)
        );
    }
}
