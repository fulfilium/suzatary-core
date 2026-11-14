//! The routes as functions of the capabilities they need, with nothing known about HTTP: a route
//! that cannot name a capability cannot use it.

use std::collections::BTreeMap;
use suzatary_core::secretary::conversations;

use chrono::NaiveDate;
use prodrome_core::fpl;
use prodrome_core::observe::Observation;
use prodrome_core::policy::Untrusted;
use suzatary_record::record::{Node, TodoEvent};
use suzatary_signatures::believed::{Believed, records};

use suzatary_signatures::chain_set::ChainSet;

use crate::query;
use crate::wire;
use suzatary_base::capability::{Clock, Datetime};
use suzatary_base::chains::{ChainName, ChainPolicy};
use suzatary_base::refusal::{Answered, Refusal, shown};
use suzatary_base::tree::{By, Classifier, Monoid};
use suzatary_core::deployment::Deployment;
use suzatary_core::mail::mailbox::{Mailbox, Query};
use suzatary_core::render::Render;
use suzatary_core::secretary::answers::Answers;
use suzatary_core::secretary::briefings::Briefings;
use suzatary_core::secretary::husks::{husked, husks, keys_of, retired};
use suzatary_core::secretary::line_form::line_header;
use suzatary_core::secretary::step::Step;
use suzatary_core::view;
use suzatary_core::wire::cards::{AboutPayload, HeldTodo, Offer, Overview, Shipped};
use suzatary_core::wire::lines::{
    Content, Frontier, Heads, NodeHeader, ObjectsPayload, ReceiptPayload, RefusedObject,
    SkeletonNode, SkeletonPayload, StoredObject,
};
use suzatary_core::wire::todo::{Entry, EntryState, Fulfillment, Neighbour};
use suzatary_core::{mail, reading};
use suzatary_store::store::Prodrome;
use suzatary_store::wire::store::SinceRead;

/// Every head, sorted, with none of them picked.
pub(crate) fn head_set<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
) -> Result<Vec<String>, Refusal> {
    Ok(store
        .tips()?
        .iter()
        .map(|h| h.as_str().to_owned())
        .collect())
}

/// Names as the wire spells them, in the set's order.
fn names(set: &std::collections::BTreeSet<prodrome_core::event::Hash>) -> Vec<String> {
    set.iter().map(|h| h.as_str().to_owned()).collect()
}

/// Every object in linearisation order, and `untrusted`, so the browser folds as the box does.
pub fn chain(store: &dyn Prodrome<TodoEvent>, chain: &ChainPolicy) -> Answered<wire::ChainPayload> {
    let objects = store.objects()?;
    Ok(wire::ChainPayload {
        heads: head_set(store)?,
        untrusted: chain.untrusted_names(),
        root: chain.root.as_str().to_owned(),
        verify: store.verify(),
        links: objects
            .iter()
            .map(|(name, envelope)| view::link(name, envelope))
            .collect(),
    })
}

/// The heads and nothing else, for a phone polling whether anything changed.
///
/// # Errors
/// The chain unreadable.
pub fn tip(store: &dyn Prodrome<TodoEvent>) -> Answered<wire::TipPayload> {
    Ok(wire::TipPayload {
        heads: head_set(store)?,
    })
}

/// Every todo the chain has mentioned, at `t`; `limit` keeps the most owed. It reads the chain's
/// name and policy alone, so the tab answers it as the box does.
pub fn fold(
    render: &dyn Render,
    name: &ChainName,
    untrusted: &Untrusted,
    set: &ChainSet<'_>,
    limit: Option<usize>,
) -> Answered<wire::FoldPayload> {
    let at = set.at();
    let (by_hash, _, rows) = rows(set, name)?;
    let next = set
        .next(name, &rows, Observation::PERCENT)
        .map_err(not_a_member)?;
    let entries: Vec<Entry> = rows
        .iter()
        .map(|row| view::entry(render, row, &by_hash, untrusted, at))
        .collect();
    let heads = set.held(name).map_err(not_a_member)?.dag.tips();
    Ok(wire::FoldPayload {
        at: fpl::iso(fpl::instant_of(at)),
        heads: names(&heads),
        entries: match limit {
            None => entries,
            Some(n) => most_owed(entries, n),
        },
        failed: set.failed(),
        next: wire::NextChange::of(next),
    })
}

/// The chain `name` as the set read it, each row priced through the whole set.
#[allow(clippy::type_complexity)]
fn rows<'s>(
    set: &'s ChainSet<'_>,
    name: &ChainName,
) -> Answered<(
    std::collections::BTreeMap<prodrome_core::event::Hash, &'s suzatary_record::record::Envelope>,
    &'s [suzatary_record::record::Node],
    Vec<suzatary_record::record::Entry>,
)> {
    let held = set.held(name).map_err(not_a_member)?;
    let rows = set.entries(name).map_err(not_a_member)?;
    let by_hash = held
        .dag
        .objects()
        .iter()
        .map(|(name, envelope)| (name.clone(), envelope))
        .collect();
    Ok((by_hash, held.nodes, rows))
}

/// A chain the set could not read, as a refusal: one it does not hold is not found.
fn not_a_member(failure: suzatary_signatures::chain_set::NotAMember) -> Refusal {
    match failure {
        suzatary_signatures::chain_set::NotAMember::Absent(_) => {
            Refusal::not_found(failure.to_string())
        }
        other => Refusal::internal(other.to_string()),
    }
}

/// The `n` open todos most in need of attention, ascending; ties keep the fold's order.
fn most_owed(entries: Vec<Entry>, n: usize) -> Vec<Entry> {
    let mut owed: Vec<(f64, Entry)> = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.state != EntryState::Open {
            continue;
        }
        if let Some(value) = entry.value.and_then(Fulfillment::number) {
            owed.push((value, entry));
        }
    }
    // `total_cmp`, so a NaN orders rather than panicking a read.
    owed.sort_by(|a, b| a.0.total_cmp(&b.0));
    owed.truncate(n);
    owed.into_iter().map(|(_, entry)| entry).collect()
}

/// One todo in full, with the routine "Build it" fires for it; an unmentioned id is a 404.
pub fn todo(
    render: &dyn Render,
    chain: &ChainPolicy,
    zone: suzatary_base::zone::Zone,
    set: &ChainSet<'_>,
    id: &str,
    builder: Option<&suzatary_core::connect::RoutineId>,
) -> Answered<wire::TodoPayload> {
    let (at, untrusted) = (set.at(), &chain.untrusted);
    let id = todo_id(id)?;
    let (by_hash, nodes, rows) = rows(set, &chain.name)?;
    let Some(row) = rows.iter().find(|row| row.key.as_str() == id) else {
        return Err(Refusal::not_found(format!(
            "no event on the chain mentions todo {}",
            shown(id)
        )));
    };
    let env = Believed::of(nodes, at, untrusted)
        .map_err(|e| Refusal::internal(e.to_string()))?
        .env;
    let refs = view::references(&rows, &by_hash).map_err(|e| Refusal::internal(e.to_string()))?;
    let back = view::backlinks(&refs);
    let neighbours = |named: Option<&Vec<String>>| -> Vec<Neighbour> {
        named
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| rows.iter().find(|row| row.key.as_str() == name))
                    .map(view::neighbour)
                    .collect()
            })
            .unwrap_or_default()
    };
    let entry = view::entry(render, row, &by_hash, untrusted, at);
    let why = reading::why(
        zone,
        view::explain(row, at, &env)
            .map(reading::Explained::of)
            .as_ref(),
    );
    Ok(wire::TodoPayload {
        todo: id.to_owned(),
        at: fpl::iso(fpl::instant_of(at)),
        entry,
        why,
        stream: row
            .stream
            .iter()
            .filter_map(|name| by_hash.get(name).map(|e| view::link(name, e)))
            .collect(),
        diffs: view::diffs(&row.stream, &by_hash),
        links: wire::TodoLinks {
            out: neighbours(refs.get(id)),
            inward: neighbours(back.get(id)),
        },
        builder: builder.map(ToString::to_string),
    })
}

/// The open todos as one tree of terms, each node valued at `t`.
pub fn tree(
    chain: &ChainPolicy,
    set: &ChainSet<'_>,
    by: wire::TreeBy,
) -> Answered<wire::TreePayload> {
    let (at, untrusted) = (set.at(), &chain.untrusted);
    let (by_hash, nodes, rows) = rows(set, &chain.name)?;
    let members = view::members(&rows, &by_hash);
    let env = Believed::of(nodes, at, untrusted)
        .map_err(|e| Refusal::internal(e.to_string()))?
        .env;
    let classifier = match by {
        wire::TreeBy::Category => Classifier::by(By::Category),
        wire::TreeBy::Urgency => Classifier::by(By::Urgency),
        wire::TreeBy::Flat => Classifier::empty(),
    }
    .combine(Classifier::by(By::Ancestry));
    Ok(wire::TreePayload {
        at: fpl::iso(fpl::instant_of(at)),
        by,
        root: view::tree_node(view::tree(&rows, &members, &classifier, at, &env), &members),
    })
}

/// The saved briefings' days, newest first.
pub fn briefings(
    briefings: &dyn Briefings,
    conversations: suzatary_core::secretary::conversations::Conversations<'_>,
) -> Answered<wire::BriefingsPayload> {
    let saved = briefings
        .all()
        .map_err(|e| Refusal::internal(e.to_string()))?;
    let mut briefs: Vec<suzatary_core::secretary::briefs::Brief> =
        conversations::list(conversations)?
            .iter()
            .flat_map(suzatary_core::secretary::briefs::briefs)
            .collect();
    briefs.sort_by(|a, b| b.date.cmp(&a.date).then(b.turn.cmp(&a.turn)));
    Ok(wire::BriefingsPayload {
        briefs: briefs
            .into_iter()
            .take(query::MAX_BRIEFINGS)
            .map(|brief| wire::BriefRow {
                date: brief.date.to_string(),
                conversation: brief.conversation.as_str().to_owned(),
                turn: u32::try_from(brief.turn).unwrap_or(u32::MAX),
                title: suzatary_core::secretary::briefs::headline(&brief.text),
                words: u32::try_from(suzatary_core::secretary::briefs::words(&brief.text))
                    .unwrap_or(u32::MAX),
                text: brief.text,
            })
            .collect(),
        briefings: saved
            .into_iter()
            .take(query::MAX_BRIEFINGS)
            .map(|(day, source)| wire::BriefingRow {
                title: briefing_title(&source, day),
                words: u32::try_from(source.split_whitespace().count()).unwrap_or(u32::MAX),
                date: day.to_string(),
            })
            .collect(),
    })
}

/// The first `= …` heading's text, else the date, without a compile.
fn briefing_title(source: &str, day: NaiveDate) -> String {
    source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('='))
        .map(|line| line.trim_start_matches('=').trim())
        .find(|text| !text.is_empty())
        .map_or_else(|| day.to_string(), str::to_owned)
}

/// One day's briefing: the secretary's brief by block, else the saved file.
pub fn briefing(
    briefings: &dyn Briefings,
    render: &dyn Render,
    conversations: suzatary_core::secretary::conversations::Conversations<'_>,
    answers: &dyn Answers,
    day: NaiveDate,
) -> Answered<wire::BriefingPayload> {
    let brief = conversations::list(conversations)?
        .iter()
        .flat_map(suzatary_core::secretary::briefs::briefs)
        .filter(|brief| brief.date == day)
        .max_by_key(|brief| brief.turn);
    if let Some(brief) = brief {
        return Ok(wire::BriefingPayload {
            date: day.to_string(),
            day: wire::BriefingDay::Brief {
                conversation: brief.conversation.as_str().to_owned(),
                turn: u32::try_from(brief.turn).unwrap_or(u32::MAX),
                blocks: suzatary_core::secretary::readings::blocks_of(answers, &brief.text),
                text: brief.text,
            },
        });
    }
    let source = briefings
        .read(day)
        .map_err(|_| Refusal::not_found(format!("no briefing for {day}")))?;
    let markup = suzatary_base::escape::Escaped::from_chain(source.clone());
    Ok(wire::BriefingPayload {
        date: day.to_string(),
        day: wire::BriefingDay::Saved {
            rich: render
                .html(&markup)
                .ok()
                .map(|html| html.as_str().to_owned()),
            source,
        },
    })
}

/// The account filter resolved against the roster; an unknown address is a refusal.
fn account<'a>(mail: &dyn Mailbox, named: Option<&'a str>) -> Answered<Option<&'a str>> {
    let Some(name) = named.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let roster = mail.accounts();
    if roster.iter().any(|held| held == name) {
        Ok(Some(name))
    } else {
        Err(Refusal::bad_request(format!(
            "no such account {} (have: {})",
            shown(name),
            roster.join(", ")
        )))
    }
}

/// The newest inbox mail by date, the agent's disposition a badge.
pub fn inbox(
    mail: &dyn Mailbox,
    named: Option<&str>,
    limit: usize,
    before: Option<Datetime>,
) -> Answered<wire::InboxPayload> {
    let account = account(mail, named)?;
    let rows = mail
        .inbox(account, limit, before)
        .map_err(|e| Refusal::internal(e.to_string()))?;
    Ok(wire::InboxPayload {
        accounts: mail.accounts(),
        messages: rows,
    })
}

/// Any notmuch query, newest first, paged by `before` like the inbox.
pub fn search(
    mail: &dyn Mailbox,
    raw: &str,
    named: Option<&str>,
    limit: usize,
    before: Option<Datetime>,
) -> Answered<wire::MailSearchPayload> {
    let account = account(mail, named)?;
    let query = Query::new(raw).map_err(|e| Refusal::bad_request(e.to_string()))?;
    let rows = mail
        .search(&query, account, limit, before)
        .map_err(|e| Refusal::internal(e.to_string()))?;
    Ok(wire::MailSearchPayload {
        q: raw.to_owned(),
        accounts: mail.accounts(),
        messages: rows,
    })
}

/// A message id: a notmuch locator or a hash prefix, never query syntax, since it is interpolated.
fn check_mail_id(value: &str) -> Answered<&str> {
    let shaped = (4..=320).contains(&value.chars().count())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._%+@!$&'/=?^{}|~<>#-".contains(c));
    if shaped {
        Ok(value)
    } else {
        Err(Refusal::bad_request(
            "a message is named by its notmuch id or its canonical hash",
        ))
    }
}

/// notmuch's conversation id: 16 hex digits today, bounded rather than pinned.
fn check_thread_id(value: &str) -> Answered<&str> {
    if (4..=32).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        Ok(value)
    } else {
        Err(Refusal::bad_request(format!(
            "a conversation id is hexadecimal, got {}",
            shown(value)
        )))
    }
}

/// One message, and the todos it produced.
pub fn message(
    mail: &dyn Mailbox,
    render: &dyn Render,
    chain: &ChainPolicy,
    set: &ChainSet<'_>,
    id: &str,
) -> Answered<wire::MailPage> {
    let id = check_mail_id(id)?;
    let found = match mail.message(id) {
        Ok(found) => found,
        Err(suzatary_core::mail_error::MailError::Absent { .. }) => {
            return Err(Refusal::not_found(gone(chain, set, id)?));
        }
        Err(other) => return Err(Refusal::internal(other.to_string())),
    };
    let todos = todos_from(render, chain, set, &found.row.hash)?;
    Ok(wire::MailPage {
        message: found,
        todos,
    })
}

/// A message no longer kept: the words a citing todo recorded.
fn gone(chain: &ChainPolicy, set: &ChainSet<'_>, id: &str) -> Result<String, Refusal> {
    let (by_hash, _, rows) = rows(set, &chain.name)?;
    let source = rows.iter().find_map(|row| {
        view::content_of(row, &by_hash)
            .and_then(|record| record.payload.source.as_ref())
            .filter(|source| {
                let cited = source.hash.as_str();
                mail::sourced_from(cited, id) || mail::sourced_from(id, cited)
            })
    });
    Ok(suzatary_core::mail::gone::Gone {
        subject: source.map(|source| source.subject.as_str()),
        sender: source.map(|source| source.sender.as_str()),
    }
    .said())
}

/// One conversation, oldest first.
pub fn thread(
    mail: &dyn Mailbox,
    render: &dyn Render,
    chain: &ChainPolicy,
    set: &ChainSet<'_>,
    id: &str,
    limit: usize,
) -> Answered<wire::ThreadPayload> {
    let id = check_thread_id(id)?;
    let found = mail.thread(id, limit).map_err(|e| match e {
        suzatary_core::mail_error::MailError::Absent { .. } => Refusal::not_found(format!(
            "{}: conversation {}",
            suzatary_core::mail::gone::SAID,
            shown(id)
        )),
        other => Refusal::internal(other.to_string()),
    })?;
    if found.is_empty() {
        return Err(Refusal::not_found(format!(
            "{}: conversation {}",
            suzatary_core::mail::gone::SAID,
            shown(id)
        )));
    }
    let mut messages = Vec::with_capacity(found.len());
    for message in found {
        let todos = todos_from(render, chain, set, &message.row.hash)?;
        messages.push(wire::MailPage { message, todos });
    }
    Ok(wire::ThreadPayload {
        thread: id.to_owned(),
        messages,
    })
}

/// Every todo whose latest `Authored` names the message.
fn todos_from(
    render: &dyn Render,
    chain: &ChainPolicy,
    set: &ChainSet<'_>,
    message_hash: &str,
) -> Answered<Vec<Entry>> {
    let (at, untrusted) = (set.at(), &chain.untrusted);
    let (by_hash, _, rows) = rows(set, &chain.name)?;
    Ok(rows
        .iter()
        .filter(|row| {
            view::content_of(row, &by_hash)
                .and_then(|record| record.payload.source.as_ref())
                .is_some_and(|source| mail::sourced_from(source.hash.as_str(), message_hash))
        })
        .map(|row| view::entry(render, row, &by_hash, untrusted, at))
        .collect())
}

/// The chain's confirmed markup worth compiling at `now`.
pub fn warmable(
    store: &dyn Prodrome<TodoEvent>,
    untrusted: &Untrusted,
    now: Datetime,
) -> Answered<Vec<suzatary_base::escape::Escaped>> {
    let records = records(&store.nodes()?, now);
    Ok(view::renderable(records.values().flatten(), untrusted))
}

/// The clock's now, as every route's default `t`.
pub fn now(clock: &dyn Clock) -> Datetime {
    clock.now()
}

/// What a pull records beside its answer.
#[derive(Clone, Copy)]
pub struct Books<'a> {
    /// Each store's epoch.
    pub epochs: &'a suzatary_signatures::epoch::Epochs,
    /// Each store's recorded frontiers.
    pub frontiers: &'a prodrome_core::store::frontier::Frontiers,
}

/// `GET /api/objects?since=<frontier>`: the objects the frontier does not reach.
///
/// # Errors
/// The store's own, and the books'.
pub fn objects_since(
    store: &dyn Prodrome<TodoEvent>,
    chain: &ChainPolicy,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
) -> Answered<ObjectsPayload> {
    answered(
        store,
        &format!("chain/{}", chain.name.as_str()),
        books,
        since,
        now,
        Cut::Whole,
        &Literals,
    )
    .map(
        |Pulled {
             snapshot, piece, ..
         }| ObjectsPayload {
            untrusted: chain.untrusted_names(),
            root: Some(chain.root.as_str().to_owned()),
            ..snapshot.payload(piece.items())
        },
    )
}

/// How much of what a caller lacks one answer carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// All of it.
    Whole,
    /// The longest run, parents first, whose literals fit this many bytes; at least one object.
    Bytes(usize),
}

impl Cut {
    /// A megabyte of literals, well under a tab's read timeout.
    pub const PIECE: Cut = Cut::Bytes(1 << 20);
}

/// `GET /api/stores/<store>/objects?since=<frontier>`: what a replica lacks, cut by `cut`.
///
/// # Errors
/// The store's own, and the books'.
pub fn store_objects_since<E: suzatary_core::stores::Store>(
    store: &dyn Prodrome<E>,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
    cut: Cut,
) -> Answered<wire::StorePayload> {
    store_answer(store, since, books, now, cut).map(|(payload, _, _)| payload)
}

/// [`store_objects_since`], with its names and graph.
fn store_answer<E: suzatary_core::stores::Store>(
    store: &dyn Prodrome<E>,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
    cut: Cut,
) -> Answered<(
    wire::StorePayload,
    Vec<prodrome_core::event::Hash>,
    std::sync::Arc<suzatary_store::store::Graph<E>>,
)> {
    let Pulled {
        snapshot,
        piece,
        graph,
    } = answered(
        store,
        &format!("store/{}", E::KIND.name()),
        books,
        since,
        now,
        cut,
        &Literals,
    )?;
    let (rest, size, weight) = (piece.rest, piece.size, piece.weight);
    let (sent, objects): (Vec<_>, Vec<_>) = piece.sent.into_iter().unzip();
    Ok((
        wire::StorePayload {
            objects: snapshot.payload(objects),
            keys: BTreeMap::new(),
            rest,
            size,
            weight,
            erased: Vec::new(),
        },
        sent,
        graph,
    ))
}

/// One object, weighed by its literal's bytes.
fn stored<E: prodrome_core::schema::Schema>(
    name: &prodrome_core::event::Hash,
    envelope: &prodrome_core::event::Envelope<E>,
) -> (StoredObject, usize) {
    let literal = prodrome_core::event::canonical_envelope(envelope);
    let bytes = literal.len();
    (
        StoredObject {
            hash: name.as_str().to_owned(),
            literal,
        },
        bytes,
    )
}

/// What a store held at one answer.
struct Snapshot {
    heads: Vec<String>,
    geneses: Vec<String>,
    epoch: u64,
    since: SinceRead,
    frontier: Frontier,
}

impl Snapshot {
    /// An objects answer, under the caller's policy.
    fn payload(self, objects: Vec<StoredObject>) -> ObjectsPayload {
        ObjectsPayload {
            heads: self.heads,
            untrusted: Vec::new(),
            root: None,
            geneses: self.geneses,
            epoch: self.epoch,
            objects,
            since: self.since,
            frontier: self.frontier,
        }
    }
}

/// What one answer of a pull sends and leaves out.
struct Piece<T> {
    sent: Vec<(prodrome_core::event::Hash, T)>,
    rest: usize,
    weight: usize,
    size: usize,
}

impl<T> Piece<T> {
    /// Nothing lacked.
    fn none() -> Self {
        Self {
            sent: Vec::new(),
            rest: 0,
            weight: 0,
            size: 0,
        }
    }

    /// The items, unnamed.
    fn items(self) -> Vec<T> {
        self.sent.into_iter().map(|(_, item)| item).collect()
    }
}

/// What a pull sends of each object.
trait Sends<E: prodrome_core::schema::Schema> {
    /// One object as sent.
    type Item;
    /// The key of the weights, read once per head set.
    const WEIGHTS: &'static str;
    /// The item and its weight.
    fn emit(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<E>,
    ) -> (Self::Item, usize);
    /// Its weight alone.
    fn weigh(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<E>,
    ) -> usize;
    /// The order to send `lacking` in, as its indices: a linear extension of the causal order,
    /// so each prefix joined to what the asker holds is closed under parents. The store's linear
    /// order unless a store says which objects a reader wants first.
    fn order(
        &self,
        _graph: &suzatary_store::store::Graph<E>,
        lacking: &[(
            &prodrome_core::event::Hash,
            &prodrome_core::event::Envelope<E>,
        )],
    ) -> Vec<usize> {
        (0..lacking.len()).collect()
    }
}

/// Each object with its literal.
struct Literals;

impl<E: prodrome_core::schema::Schema> Sends<E> for Literals {
    type Item = StoredObject;
    const WEIGHTS: &'static str = "weights of literals";

    fn emit(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<E>,
    ) -> (StoredObject, usize) {
        stored(name, envelope)
    }

    fn weigh(
        &self,
        _: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<E>,
    ) -> usize {
        prodrome_core::event::canonical_envelope(envelope).len()
    }
}

/// Each line as its skeleton node.
struct Nodes;

impl Sends<suzatary_core::secretary::line_event::LineEvent> for Nodes {
    type Item = SkeletonNode;
    const WEIGHTS: &'static str = "weights of nodes";

    fn emit(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<suzatary_core::secretary::line_event::LineEvent>,
    ) -> (SkeletonNode, usize) {
        // Sized by its file once cut ([`held_sizes`]), so its literal is not printed here.
        let node = node_of(name, envelope, 0);
        let weight = weight(&node);
        (node, weight)
    }

    fn weigh(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<suzatary_core::secretary::line_event::LineEvent>,
    ) -> usize {
        weight(&node_of(name, envelope, 0))
    }

    /// The newest conversation first: conversations by their latest line, latest first, each in
    /// the store's linear order, every object after the ancestors it lacks; then what is no line.
    /// So a device draws the conversation it opens to from the first piece.
    fn order(
        &self,
        graph: &suzatary_store::store::Graph<suzatary_core::secretary::line_event::LineEvent>,
        lacking: &[(
            &prodrome_core::event::Hash,
            &prodrome_core::event::Envelope<suzatary_core::secretary::line_event::LineEvent>,
        )],
    ) -> Vec<usize> {
        let latest = graph.reading("latest line of each conversation", |graph| {
            let mut latest: std::collections::HashMap<
                suzatary_base::conversation_id::ConversationId,
                fpl::Instant,
            > = std::collections::HashMap::new();
            for (_, envelope) in graph.objects() {
                if let Some(line) = envelope.event() {
                    let at = fpl::instant_of(line.at());
                    latest
                        .entry(line.conversation().clone())
                        .and_modify(|was| *was = (*was).max(at))
                        .or_insert(at);
                }
            }
            latest
        });
        let mut wanted: Vec<_> = lacking
            .iter()
            .enumerate()
            .map(|(at, (_, envelope))| {
                let conversation = envelope
                    .event()
                    .map(suzatary_core::secretary::line_event::LineEvent::conversation);
                let newest = conversation.and_then(|id| latest.get(id)).copied();
                (std::cmp::Reverse(newest), conversation, at)
            })
            .collect();
        wanted.sort_unstable();
        closed_order(lacking, wanted.into_iter().map(|(_, _, at)| at))
    }
}

/// `wanted`, a permutation of `lacking`'s indices, made a linear extension: each object is sent
/// after every ancestor of it `lacking` holds, those in `lacking`'s own order, which is one.
fn closed_order<E>(
    lacking: &[(
        &prodrome_core::event::Hash,
        &prodrome_core::event::Envelope<E>,
    )],
    wanted: impl Iterator<Item = usize>,
) -> Vec<usize> {
    let at: std::collections::HashMap<&prodrome_core::event::Hash, usize> = lacking
        .iter()
        .enumerate()
        .map(|(at, (name, _))| (*name, at))
        .collect();
    let mut sent = vec![false; lacking.len()];
    let mut order = Vec::with_capacity(lacking.len());
    for one in wanted {
        if sent[one] {
            continue;
        }
        let mut before = Vec::new();
        let mut pending = vec![one];
        while let Some(next) = pending.pop() {
            if sent[next] {
                continue;
            }
            sent[next] = true;
            before.push(next);
            pending.extend(
                prodrome_core::event::parents_of(lacking[next].1)
                    .iter()
                    .filter_map(|parent| at.get(parent).copied())
                    .filter(|&parent| !sent[parent]),
            );
        }
        before.sort_unstable();
        order.extend(before);
    }
    order
}

/// An answer of `store`, read under its epoch's lock so the epoch and the objects are one
/// snapshot: every head, the prodromes it serves, and what `sends` makes of each object the
/// frontier `since` names does not reach, cut as `cut` says by the weight it gives each, in the
/// order `sends` gives, a linear extension, so a receiver can apply them as they arrive. The frontier the asker holds
/// once it has taken the answer is recorded, and named in it. The store is read through the graph
/// it keeps by its heads, so an answer at heads already read reads no object; one to a `since` at
/// the store's heads also walks none, and sends nothing.
fn answered<E, S>(
    store: &dyn Prodrome<E>,
    key: &str,
    books: Books<'_>,
    since: &prodrome_core::store::frontier::Since,
    now: Datetime,
    cut: Cut,
    sends: &S,
) -> Answered<Pulled<E, S::Item>>
where
    E: prodrome_core::schema::Schema + Send + Sync + 'static,
    S: Sends<E>,
{
    let (epoch, (read, frontier, piece, graph)) = books.epochs.served(key, now, || {
        let graph = store.graph()?;
        let seen = suzatary_signatures::epoch::Seen {
            geneses: names(graph.geneses()).into_iter().collect(),
            heads: names(graph.heads()).into_iter().collect(),
            held: std::sync::Arc::clone(graph.names()),
        };
        let lacking = books
            .frontiers
            .answer(key, since, graph.dag())
            .map_err(suzatary_store::store::StoreError::from)?;
        let read = since_read(lacking.reached());
        let mut piece = Piece::none();
        let mut full = false;
        // Weighed once per head set, so a pull of many pieces weighs each object once.
        let weights = graph.reading(S::WEIGHTS, |graph| {
            graph
                .objects()
                .map(|(name, envelope)| (name.clone(), sends.weigh(name, envelope)))
                .collect::<std::collections::HashMap<_, _>>()
        });
        let order = sends.order(&graph, lacking.lacking());
        for (name, envelope) in order.into_iter().map(|at| lacking.lacking()[at]) {
            piece.rest += 1;
            if full {
                piece.size += weights
                    .get(name)
                    .copied()
                    .unwrap_or_else(|| sends.weigh(name, envelope));
                continue;
            }
            let (item, weight) = sends.emit(name, envelope);
            match cut {
                Cut::Bytes(most) if piece.weight + weight > most && !piece.sent.is_empty() => {
                    full = true;
                }
                Cut::Whole | Cut::Bytes(_) => {
                    piece.sent.push((name.clone(), item));
                    piece.weight += weight;
                }
            }
            piece.size += weight;
        }
        piece.rest -= piece.sent.len();
        // The pieces are sent in `order`, not as a prefix of the linear order, so the frontier is
        // what the asker held joined to the closure of what it was sent.
        let sent: Vec<_> = piece.sent.iter().map(|(name, _)| name.clone()).collect();
        let frontier = frontier_of(
            &lacking
                .record_with(sent)
                .map_err(suzatary_store::store::StoreError::from)?,
        );
        Ok((seen, (read, frontier, piece, std::sync::Arc::clone(&graph))))
    })?;
    Ok(Pulled {
        snapshot: Snapshot {
            heads: names(graph.heads()),
            geneses: names(graph.geneses()),
            epoch,
            since: read,
            frontier,
        },
        piece,
        graph,
    })
}

/// One answer, and its graph.
struct Pulled<E: prodrome_core::schema::Schema, T> {
    snapshot: Snapshot,
    piece: Piece<T>,
    graph: std::sync::Arc<suzatary_store::store::Graph<E>>,
}

/// Whether the box knew the frontier a pull named.
fn since_read(reached: &prodrome_core::store::frontier::Reached) -> SinceRead {
    match reached {
        prodrome_core::store::frontier::Reached::Everything
        | prodrome_core::store::frontier::Reached::Known(_) => SinceRead::Known,
        prodrome_core::store::frontier::Reached::Unknown => SinceRead::Unknown,
    }
}

/// The frontier an answer recorded.
fn frontier_of(recorded: &prodrome_core::store::frontier::Recorded) -> Frontier {
    Frontier {
        digest: recorded.token.as_str().to_owned(),
        count: recorded.count,
    }
}

/// `GET /api/stores/lines/skeleton?since=<heads>`: the lines' pull, as nodes.
///
/// # Errors
/// The stores' own.
pub fn lines_skeleton(
    conversations: suzatary_core::secretary::conversations::Conversations<'_>,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
    cut: Cut,
) -> Answered<SkeletonPayload> {
    let Pulled {
        snapshot,
        piece,
        graph,
    } = answered(
        conversations.lines,
        "store/lines",
        books,
        since,
        now,
        cut,
        &Nodes,
    )?;
    let (rest, size, weight) = (piece.rest, piece.size, piece.weight);
    let (sent, nodes): (Vec<_>, Vec<_>) = piece.sent.into_iter().unzip();
    let keys = if sent.is_empty() {
        BTreeMap::new()
    } else {
        keys_of(conversations, &graph, &retired(&graph), &sent)?
    };
    let husked = husked(&graph, &keys, &sent);
    // A replica that lacks nothing holds the store's heads already.
    let heads = match (&snapshot.since, sent.is_empty() && rest == 0) {
        (SinceRead::Known, true) => Heads::Held,
        (SinceRead::Known | SinceRead::Unknown, _) => Heads::These {
            heads: snapshot.heads,
        },
    };
    Ok(SkeletonPayload {
        heads,
        geneses: snapshot.geneses,
        epoch: snapshot.epoch,
        nodes: held_sizes(conversations.lines, &sent, nodes)?
            .into_iter()
            .zip(&sent)
            .map(|(node, name)| match husked.get(name) {
                Some(husk) => SkeletonNode {
                    size: husk.len(),
                    ..node
                },
                None => node,
            })
            .collect(),
        since: snapshot.since,
        frontier: snapshot.frontier,
        keys,
        rest,
        size,
        weight,
    })
}

/// Each node, named `names`, sized by the bytes [`content`] sends of it, which are the store's own
/// print and not its reprint, so a tab that batches its asks by these sizes never asks for more
/// than one answer carries.
///
/// A node the store holds no print of refuses the piece rather than being sized by its reprint.
/// That holds while every object has its file; a prune of the box's disk that keeps a husk in a
/// literal's place has to answer this case again.
fn held_sizes(
    lines: &dyn Prodrome<suzatary_core::secretary::line_event::LineEvent>,
    names: &[prodrome_core::event::Hash],
    nodes: Vec<SkeletonNode>,
) -> Answered<Vec<SkeletonNode>> {
    if names.is_empty() {
        return Ok(nodes);
    }
    let sizes = lines.sizes(names)?;
    nodes
        .into_iter()
        .zip(sizes)
        .map(|(node, size)| match size {
            Some(size) => Ok(SkeletonNode { size, ..node }),
            None => Err(Refusal::internal(format!(
                "the lines hold no print of {}",
                node.hash
            ))),
        })
        .collect()
}

/// One line's node.
#[must_use]
pub fn skeleton_node(
    name: &prodrome_core::event::Hash,
    envelope: &prodrome_core::event::Envelope<suzatary_core::secretary::line_event::LineEvent>,
) -> SkeletonNode {
    node_of(
        name,
        envelope,
        prodrome_core::event::canonical_envelope(envelope).len(),
    )
}

/// One line's node.
fn node_of(
    name: &prodrome_core::event::Hash,
    envelope: &prodrome_core::event::Envelope<suzatary_core::secretary::line_event::LineEvent>,
    size: usize,
) -> SkeletonNode {
    let header = match envelope.event() {
        Some(line) => line_header(line),
        None => NodeHeader::Structure {
            kind: constructor(envelope).to_owned(),
        },
    };
    SkeletonNode {
        hash: name.as_str().to_owned(),
        parents: prodrome_core::event::parents_of(envelope)
            .iter()
            .map(|parent| parent.as_str().to_owned())
            .collect(),
        size,
        header,
    }
}

/// An envelope's constructor.
fn constructor<E>(envelope: &prodrome_core::event::Envelope<E>) -> &'static str {
    use prodrome_core::event::Envelope;
    match envelope {
        Envelope::Sealed { .. } => "Sealed",
        Envelope::Woven { .. } => "Woven",
        Envelope::Genesis(_) => "Genesis",
        Envelope::Change(_) => "Change",
        Envelope::Snapshot(_) => "Snapshot",
        Envelope::Signed(_) => "Signed",
        Envelope::KeyAdded(_) => "KeyAdded",
        Envelope::KeyRevoked(_) => "KeyRevoked",
    }
}

/// About how many bytes a node is on the wire.
fn weight(node: &SkeletonNode) -> usize {
    const AROUND: usize = 96;
    // A step's tag, and the count two of them carry; the keys two others name are counted apart.
    const STEP: usize = 32;
    let header = match &node.header {
        NodeHeader::Structure { kind } => kind.len(),
        NodeHeader::Line {
            conversation,
            at,
            kind,
            keys,
            step,
        } => {
            conversation.len()
                + at.len()
                + kind.len()
                + keys.iter().map(|key| key.len() + 3).sum::<usize>()
                + STEP
                + match step {
                    Step::Resealed { of, from } => of.as_str().len() + from.as_str().len(),
                    Step::Erased { keys } => {
                        keys.iter().map(|key| key.as_str().len() + 3).sum::<usize>()
                    }
                    Step::Opened
                    | Step::Asks
                    | Step::Noted
                    | Step::Answers
                    | Step::Rewound { .. }
                    | Step::Proposed
                    | Step::Coding { .. }
                    | Step::Titled
                    | Step::Still => 0,
                }
        }
    };
    AROUND + node.hash.len() + node.parents.iter().map(|p| p.len() + 3).sum::<usize>() + header
}

/// The most bytes of literals one content answer of more than one hash carries; a single object
/// is answered whatever its size.
pub const MOST_CONTENT_BYTES: usize = 1 << 20;

/// The content questions for `nodes`, each within an answer's bounds.
#[must_use]
pub fn asks<'n>(nodes: impl IntoIterator<Item = (&'n str, usize)>) -> Vec<Vec<&'n str>> {
    let mut nodes: Vec<(&str, usize)> = nodes.into_iter().collect();
    nodes.sort_unstable();
    let mut asks: Vec<Vec<&str>> = Vec::new();
    let mut bytes = 0usize;
    for (hash, size) in nodes {
        match asks.last_mut() {
            Some(ask) if ask.len() < query::MOST_HASHES && bytes + size <= MOST_CONTENT_BYTES => {
                ask.push(hash);
                bytes += size;
            }
            Some(_) | None => {
                asks.push(vec![hash]);
                bytes = size;
            }
        }
    }
    asks
}

/// `GET …/content?hashes=`: the literal of each object `hashes` names, from `store` alone.
///
/// # Errors
/// [`Refusal::not_found`] for a hash the store does not hold; [`Refusal::bad_request`] past the
/// bound; the store's own.
pub fn content<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
    hashes: &query::Hashes,
) -> Answered<wire::ContentPayload> {
    answer(store, &*store.graph()?, hashes, &BTreeMap::new())
}

/// [`content`] of the lines, each line whose every key the box retires as its husk.
pub fn lines_content(
    conversations: suzatary_core::secretary::conversations::Conversations<'_>,
    hashes: &query::Hashes,
) -> Answered<wire::ContentPayload> {
    let graph = conversations.lines.graph()?;
    let names: Vec<prodrome_core::event::Hash> = hashes.iter().cloned().collect();
    let printed = husks(conversations, &graph, &names)?;
    answer(conversations.lines, &graph, hashes, &printed)
}

/// Each object `hashes` names: its husk where `husks` holds one, else its literal.
fn answer<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
    graph: &suzatary_store::store::Graph<E>,
    hashes: &query::Hashes,
    husks: &BTreeMap<prodrome_core::event::Hash, String>,
) -> Answered<wire::ContentPayload> {
    let read: Vec<prodrome_core::event::Hash> = hashes
        .iter()
        .filter(|name| !husks.contains_key(*name) && graph.dag().get(name).is_some())
        .cloned()
        .collect();
    let mut literals: BTreeMap<_, _> = read.iter().zip(store.literals(&read)?).collect();
    let mut unheld = Vec::new();
    let sent: Vec<Content> = hashes
        .iter()
        .filter_map(|name| {
            let hash = name.as_str().to_owned();
            match (husks.get(name), literals.remove(name).flatten()) {
                (Some(husk), _) => Some(Content::Husk {
                    hash,
                    husk: husk.clone(),
                }),
                (None, Some(literal)) => Some(Content::Literal { hash, literal }),
                (None, None) => {
                    unheld.push(hash);
                    None
                }
            }
        })
        .collect();
    if !unheld.is_empty() {
        return Err(Refusal::not_found(format!(
            "this store holds no object named {}",
            unheld.join(", ")
        )));
    }
    let bytes: usize = sent
        .iter()
        .map(|object| match object {
            Content::Literal { literal: text, .. } | Content::Husk { husk: text, .. } => text.len(),
        })
        .sum();
    if sent.len() > 1 && bytes > MOST_CONTENT_BYTES {
        return Err(Refusal::bad_request(format!(
            "these {} objects are {bytes} bytes, past the {MOST_CONTENT_BYTES} one answer carries: ask for fewer at a time, by the sizes the skeleton gives",
            sent.len()
        )));
    }
    Ok(wire::ContentPayload { objects: sent })
}

/// `GET /api/waiting`: the Inbox badge's count, the lines read through the box's memoised fold.
///
/// # Errors
/// A store that does not read, a chain the set does not hold, and lines the fold refuses.
pub fn waiting(
    inbox: &suzatary_core::secretary::inbox::Fold,
    conversations: suzatary_core::secretary::conversations::ReadConversations<'_>,
    set: &ChainSet<'_>,
    chains: &[ChainName],
) -> Answered<wire::WaitingPayload> {
    let proposals = suzatary_core::secretary::readings::waiting(inbox, set).len();
    let mut decisions = 0usize;
    for chain in chains {
        let (by_hash, _, rows) = rows(set, chain)?;
        decisions += rows
            .iter()
            .filter(|row| {
                view::content_of(row, &by_hash).is_some_and(|record| {
                    record.payload.is_decision()
                        && EntryState::from(suzatary_signatures::believed::one_outcome(
                            row.outcome(),
                        )) == EntryState::Open
                })
            })
            .count();
    }
    let mut unplaced = BTreeMap::new();
    for (id, read) in
        conversations.fold(suzatary_core::secretary::held_lines::HeldLines::transcripts)?
    {
        let read = read.map_err(|refused| Refusal::internal(refused.to_string()))?;
        let lines = suzatary_core::secretary::readings::unplaced(&read.stored().lines(inbox));
        if !lines.is_empty() {
            unplaced.insert(id.as_str().to_owned(), lines);
        }
    }
    Ok(wire::WaitingPayload {
        count: proposals + decisions + unplaced.values().map(Vec::len).sum::<usize>(),
        unplaced,
    })
}

/// The most objects one post may carry; more is a refusal, never a silent truncation.
pub const MAX_POSTED: usize = 512;

/// `POST /api/objects`: the one chain write path, decided under the write lock.
pub fn receive_objects(
    store: &dyn Prodrome<TodoEvent>,
    render: &dyn Render,
    chain: &ChainPolicy,
    posted: &Posted<'_>,
    frontiers: &prodrome_core::store::frontier::Frontiers,
    actor: &prodrome_core::event::Actor,
) -> Answered<(
    ReceiptPayload,
    Vec<suzatary_core::secretary::act_lines::ActLine>,
)> {
    let Posted {
        objects: posted,
        since,
    } = *posted;
    if posted.len() > MAX_POSTED {
        return Err(Refusal::bad_request(format!(
            "at most {MAX_POSTED} objects per post, got {}",
            posted.len()
        )));
    }
    let _held = store.locked()?;
    let objects = store.objects()?;
    let outcome = suzatary_core::sync::receive(
        &suzatary_core::sync::Objects::of(&objects),
        posted,
        actor,
        &chain.root,
    );
    let taken: Vec<TodoEvent> = outcome
        .taken
        .iter()
        .filter_map(|(_, literal)| {
            prodrome_core::event::parse_envelope(
                &suzatary_record::record::TodoVocabulary::default(),
                literal,
            )
            .ok()
        })
        .filter_map(|envelope| envelope.event().cloned())
        .collect();
    // A sealed act's note is known here only on adoption; a record's verb reads the record before.
    let shown = if taken
        .iter()
        .any(|event| matches!(event, TodoEvent::Authored(_)))
    {
        store.nodes()?
    } else {
        std::sync::Arc::default()
    };
    let heard = suzatary_core::secretary::act_lines::heard_from_objects(
        &taken,
        actor,
        &chain.name,
        &|posted| suzatary_core::todos::current(&shown, posted.at, posted.todo.as_str()),
    );
    store.accept(&outcome.taken)?;
    // Render what landed before answering; `renders` keeps untrusted records uncompiled.
    let fresh: Vec<suzatary_base::escape::Escaped> = taken
        .iter()
        .filter_map(|event| match event {
            TodoEvent::Authored(record) => Some(record.clone()),
            _ => None,
        })
        .filter(|record| view::renders(record, &chain.untrusted))
        .flat_map(|record| {
            view::rich_sources(&record)
                .into_iter()
                .map(|(_, source)| source)
                .collect::<Vec<_>>()
        })
        .collect();
    if !fresh.is_empty() {
        render.warm(&fresh);
    }
    let key = format!("chain/{}", chain.name.as_str());
    Ok((receipt(store, &outcome, (&key, since, frontiers))?, heard))
}

/// What a post carries.
#[derive(Clone, Copy)]
pub struct Posted<'a> {
    /// Canonical prints.
    pub objects: &'a [String],
    /// The poster's frontier.
    pub since: &'a prodrome_core::store::frontier::Since,
}

/// Who posts to a store a tab writes, and the chains it is trusted on.
#[derive(Clone, Copy)]
pub struct Writer<'a> {
    /// The gate's actor.
    pub actor: &'a prodrome_core::event::Actor,
    /// Every chain.
    pub chains: &'a [&'a dyn Prodrome<suzatary_record::record::TodoEvent>],
    /// The deployment's root.
    pub root: &'a prodrome_core::sign::PublicKey,
}

/// Where and when a post records its refusals.
#[derive(Clone, Copy)]
pub struct Recording<'a> {
    /// The asks store.
    pub asks: &'a dyn Prodrome<suzatary_core::asks::AskEvent>,
    /// Now.
    pub at: Datetime,
}

/// `POST /api/stores/<store>/objects`: decided under the store's lock, taken in, then each
/// proven refusal recorded once in the asks under its own lock.
///
/// # Errors
/// More than `MAX_POSTED` objects, or a store not read or not written.
pub fn receive_changes<E: suzatary_core::stores::Store>(
    store: &dyn Prodrome<E>,
    posted: &Posted<'_>,
    frontiers: &prodrome_core::store::frontier::Frontiers,
    writer: Writer<'_>,
    recording: Recording<'_>,
) -> Answered<ReceiptPayload>
where
    E::Vocabulary: Default,
{
    let Posted {
        objects: posted,
        since,
    } = *posted;
    if posted.len() > MAX_POSTED {
        return Err(Refusal::bad_request(format!(
            "at most {MAX_POSTED} objects per post, got {}",
            posted.len()
        )));
    }
    let received = {
        let _held = store.locked()?;
        // Read under the lock, so a revocation that lands before it is seen by this post.
        let trusting = suzatary_signatures::signatures::Trusting::held(writer.chains, writer.root)?;
        let held = store.objects()?.into_iter().map(|(name, _)| name).collect();
        let received =
            suzatary_core::sync::receive_changes::<E>(&held, posted, writer.actor, &trusting);
        store.accept(&received.outcome.taken)?;
        received
    };
    record(&received.recorded, E::KIND, recording)?;
    let key = format!("store/{}", E::KIND.name());
    receipt(store, &received.outcome, (&key, since, frontiers))
}

/// Each of `refused` recorded once in the asks.
fn record(
    refused: &[(prodrome_core::event::Hash, suzatary_core::sync::Reason)],
    store: suzatary_core::stores::Kind,
    Recording { asks, at }: Recording<'_>,
) -> Result<(), Refusal> {
    if refused.is_empty() {
        return Ok(());
    }
    let _held = asks.locked()?;
    let objects = asks.objects()?;
    let mut recorded = suzatary_core::receipt::refusals(
        objects.iter().map(|(name, object)| (name, object)),
        &store,
    );
    for (name, reason) in refused {
        if recorded.insert(name.clone(), reason.said()).is_none() {
            asks.append(suzatary_core::receipt::refused(
                name.clone(),
                at,
                suzatary_core::asks::keeper(),
                store,
                reason.said(),
            ))?;
        }
    }
    Ok(())
}

/// A post's receipt.
fn receipt<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
    outcome: &suzatary_core::sync::Outcome,
    (key, since, frontiers): (
        &str,
        &prodrome_core::store::frontier::Since,
        &prodrome_core::store::frontier::Frontiers,
    ),
) -> Answered<ReceiptPayload> {
    let objects = store.objects()?;
    let dag: prodrome_core::dag::Dag<E> = objects.into_iter().collect();
    let lacking = frontiers
        .answer(key, since, &dag)
        .map_err(suzatary_store::store::StoreError::from)?;
    let read = since_read(lacking.reached());
    let posted = outcome
        .taken
        .iter()
        .map(|(name, _)| name)
        .chain(&outcome.already)
        .cloned();
    let frontier = frontier_of(
        &lacking
            .record_with(posted)
            .map_err(suzatary_store::store::StoreError::from)?,
    );
    Ok(ReceiptPayload {
        since: read,
        frontier,
        heads: head_set(store)?,
        adopted: outcome
            .taken
            .iter()
            .map(|(name, _)| name.as_str().to_owned())
            .collect(),
        already: outcome
            .already
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect(),
        refused: outcome
            .refused
            .iter()
            .map(|refused| RefusedObject {
                hash: refused
                    .name
                    .as_ref()
                    .map_or_else(String::new, |name| name.as_str().to_owned()),
                reason: refused.reason.said(),
                kind: refused.reason.kind(),
            })
            .collect(),
    })
}

/// `GET /api/about`: the release this binary runs and the todos it completed; `dev` unstamped.
///
/// # Errors
/// The plan's chain held and not read.
pub fn about(
    released: Option<&suzatary_core::release::Released>,
    build: &str,
    plan: &suzatary_core::release::Plan,
    set: &ChainSet<'_>,
) -> Answered<AboutPayload> {
    let completes = released.map_or(&[][..], |r| r.completes.as_slice());
    let mut held = held_todos(plan, set, completes)?.unwrap_or_default();
    Ok(AboutPayload {
        version: released.map_or_else(|| "dev".to_owned(), |r| r.version.to_string()),
        build: build.to_owned(),
        completes: completes
            .iter()
            .map(|todo| Shipped {
                todo: todo.as_str().to_owned(),
                held: held.remove(todo.as_str()),
            })
            .collect(),
    })
}

/// Each of `todos` as `plan`'s chain reads it now; `None` where the set lacks the chain.
fn held_todos(
    plan: &suzatary_core::release::Plan,
    set: &ChainSet<'_>,
    todos: &[prodrome_core::event::TodoId],
) -> Answered<Option<std::collections::BTreeMap<String, HeldTodo>>> {
    let name = suzatary_base::chains::ChainName::new(plan.chain())
        .map_err(|e| Refusal::internal(e.to_string()))?;
    match set.held(&name) {
        Ok(_) => {}
        Err(suzatary_signatures::chain_set::NotAMember::Absent(_)) => return Ok(None),
        Err(failure) => return Err(not_a_member(failure)),
    }
    let (by_hash, _, rows) = rows(set, &name)?;
    let wanted: std::collections::BTreeSet<&str> = todos
        .iter()
        .map(prodrome_core::event::TodoId::as_str)
        .collect();
    Ok(Some(
        rows.iter()
            .filter(|entry| wanted.contains(entry.key.as_str()))
            .map(|entry| {
                let title = view::content_of(entry, &by_hash).map_or_else(
                    || entry.key.as_str().to_owned(),
                    |record| suzatary_base::escape::markup_text(record.payload.body.as_str()),
                );
                (
                    entry.key.as_str().to_owned(),
                    HeldTodo {
                        row: view::neighbour(entry),
                        title,
                    },
                )
            })
            .collect(),
    ))
}

/// `GET /api/releases`: `plan`'s releases as `set` reads them.
///
/// # Errors
/// The chain held and not read.
pub fn releases(
    plan: &suzatary_core::release::Plan,
    set: &ChainSet<'_>,
) -> Answered<wire::ReleasesPayload> {
    let name = suzatary_base::chains::ChainName::new(plan.chain())
        .map_err(|e| Refusal::internal(e.to_string()))?;
    let held = match set.held(&name) {
        Ok(held) => held,
        Err(suzatary_signatures::chain_set::NotAMember::Absent(_)) => {
            return Ok(wire::ReleasesPayload {
                chain: plan.chain().to_owned(),
                reads: suzatary_core::release::Counted {
                    major: plan.base(),
                    minor: 0,
                }
                .to_string(),
                releases: Vec::new(),
            });
        }
        Err(failure) => return Err(not_a_member(failure)),
    };
    let current = Believed::of(held.nodes, set.at(), held.untrusted)
        .map_err(|e| Refusal::internal(e.to_string()))?
        .specs;
    let reading = suzatary_core::release::Reading::of(
        plan,
        suzatary_core::release::binding(held.nodes, held.untrusted),
        set.at(),
        &current,
    );
    let todos: Vec<prodrome_core::event::TodoId> = reading
        .releases
        .iter()
        .flat_map(|release| {
            std::iter::once(release.todo.clone())
                .chain(release.items.iter().map(|item| item.todo.clone()))
        })
        .collect();
    let by_id = held_todos(plan, set, &todos)?.unwrap_or_default();
    let held_todo = |todo: &prodrome_core::event::TodoId| by_id.get(todo.as_str()).cloned();
    Ok(wire::ReleasesPayload {
        chain: plan.chain().to_owned(),
        reads: reading.counted.to_string(),
        releases: reading
            .releases
            .iter()
            .map(|release| wire::ReleaseRow {
                todo: release.todo.as_str().to_owned(),
                held: held_todo(&release.todo),
                version: release.version.to_string(),
                lot: release.lot,
                place: release.place,
                items: release
                    .items
                    .iter()
                    .map(|item| wire::ReleaseItem {
                        todo: item.todo.as_str().to_owned(),
                        held: held_todo(&item.todo),
                        lot: item.lot,
                        named: item.named,
                    })
                    .collect(),
            })
            .collect(),
    })
}

/// `GET /api/<chain>/me`: the login and the actor it writes as; `null` for a viewer.
#[must_use]
pub fn me(
    deployment: &Deployment,
    chain: &ChainPolicy,
    admitted: &suzatary_signatures::signatures::Admitted,
) -> wire::MePayload {
    let login = admitted.login();
    wire::MePayload {
        login: login.map_or_else(String::new, |login| login.as_str().to_owned()),
        actor: login
            .and_then(|login| deployment.actor_of(login))
            .filter(|actor| chain.roster.contains(*actor))
            .map(|actor| actor.as_str().to_owned()),
        chains: deployment
            .chains
            .names()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        categories: deployment.category_order(),
        zone: deployment.zone.name().to_owned(),
    }
}

/// `POST /api/author`: build an event and append nothing; the replica seals it.
pub fn author(
    caps: &crate::server::Server<'_>,
    deployment: &Deployment,
    chain: &ChainPolicy,
    request: &wire::Authoring,
    actor: &prodrome_core::event::Actor,
) -> Answered<wire::AuthoredPayload> {
    let nodes = caps.secretary.chain.nodes()?;
    let at = caps.secretary.clock.now();
    let typeset = caps.secretary.render;
    let (todo, event) = match request {
        wire::Authoring::Add(new) => {
            let (todo, event, revision) = add(deployment, chain, &nodes, new, at, actor, typeset)?;
            return Ok(wire::AuthoredPayload {
                todo,
                event: prodrome_core::event::canonical(&event),
                revision: revision.as_ref().map(prodrome_core::event::canonical),
            });
        }
        wire::Authoring::Update(update) => {
            let wire::UpdateTodo { todo, patch, note } = &**update;
            let id = todo_id(todo)?.to_owned();
            let Some(before) = suzatary_core::todos::current(&nodes, at, &id) else {
                return Err(Refusal::not_found(format!(
                    "no todo with id {} on the chain",
                    shown(&id)
                )));
            };
            capped_patch(patch)?;
            let known: Vec<&str> = deployment.categories.keys().map(String::as_str).collect();
            let event = suzatary_core::authoring::update_todo(
                &before,
                patch,
                suzatary_base::zone::Moment {
                    at,
                    zone: deployment.zone,
                },
                actor.as_str(),
                &known,
                note,
                typeset,
            )?;
            (id, event)
        }
        wire::Authoring::Confirm(wire::ConfirmTodo { todo, note }) => {
            let id = todo_id(todo)?.to_owned();
            let Some(before) = suzatary_core::todos::current(&nodes, at, &id) else {
                return Err(Refusal::not_found(format!(
                    "the chain holds no content for todo {}",
                    shown(&id)
                )));
            };
            let event = suzatary_core::authoring::confirm_todo(
                &before,
                at,
                actor.as_str(),
                &chain.untrusted,
                note,
            )?;
            (id, event)
        }
        wire::Authoring::Undo(todo) => {
            let id = todo_id(todo)?.to_owned();
            let stream = view::stream_of(&nodes, &id);
            let event =
                suzatary_core::authoring::undo(&stream, &id, at, actor.as_str(), &chain.untrusted)?;
            (id, event)
        }
        wire::Authoring::Redo(todo) => {
            let id = todo_id(todo)?.to_owned();
            let stream = view::stream_of(&nodes, &id);
            let event =
                suzatary_core::authoring::redo(&stream, &id, at, actor.as_str(), &chain.untrusted)?;
            (id, event)
        }
    };
    Ok(wire::AuthoredPayload {
        todo,
        event: prodrome_core::event::canonical(&event),
        revision: None,
    })
}

/// A new todo through the one authoring path, under an id no event has mentioned.
fn add(
    deployment: &Deployment,
    chain: &ChainPolicy,
    nodes: &[Node],
    new: &wire::NewTodo,
    at: Datetime,
    actor: &prodrome_core::event::Actor,
    typeset: &dyn suzatary_base::capability::Typeset,
) -> Result<(String, TodoEvent, Option<TodoEvent>), Refusal> {
    let mut fields = new.fields.clone();
    capped("body", &fields.body, MAX_BODY_CHARS)?;
    capped("rationale", &fields.rationale, MAX_RATIONALE_CHARS)?;
    capped("detail", &fields.detail, MAX_DETAIL_CHARS)?;
    capped("waiting_on", &fields.waiting_on, MAX_WAITING_ON_CHARS)?;
    let known: Vec<&str> = deployment.categories.keys().map(String::as_str).collect();
    // The app always files a todo somewhere; "" is a category only a kind that sets its own has.
    if !known.contains(&fields.category.as_str()) {
        let mut sorted = known.clone();
        sorted.sort_unstable();
        return Err(Refusal::bad_request(format!(
            "category must be one of {sorted:?}, got {}",
            shown(&fields.category)
        )));
    }
    if fields.rationale.trim().is_empty() {
        APP_RATIONALE.clone_into(&mut fields.rationale);
    }
    let taken: std::collections::BTreeSet<String> = nodes
        .iter()
        .filter_map(|node| node.event())
        .map(|e| e.todo().as_str().to_owned())
        .collect();
    let todo = match &new.id {
        None => suzatary_core::authoring::fresh_id(
            &suzatary_core::authoring::slug(&fields.body)?,
            &taken,
        )?,
        Some(named) => {
            let named = todo_id(named)?.to_owned();
            if taken.contains(&named) {
                return Err(Refusal::conflict(format!(
                    "todo {} already exists — the chain has mentioned it before",
                    shown(&named)
                )));
            }
            named
        }
    };
    let new_todo = suzatary_core::authoring::NewTodo {
        todo: todo.clone(),
        fields,
        source: None,
        subtodos: Vec::new(),
    };
    let Some(parent) = &new.parent else {
        let now = suzatary_base::zone::Moment {
            at,
            zone: deployment.zone,
        };
        let event =
            suzatary_core::authoring::build_todo(&new_todo, now, actor.as_str(), &known, typeset)?;
        return Ok((todo, event, None));
    };
    let Some(record) = suzatary_core::todos::current(nodes, at, todo_id(parent)?) else {
        return Err(Refusal::not_found(format!(
            "no todo with id {} on the chain to be a parent",
            shown(parent)
        )));
    };
    let believed =
        Believed::of(nodes, at, &chain.untrusted).map_err(|e| Refusal::internal(e.to_string()))?;
    let (event, revision) = suzatary_core::authoring::build_subtodo(
        &new_todo,
        &record,
        &believed.linkable(),
        suzatary_base::zone::Moment {
            at,
            zone: deployment.zone,
        },
        actor.as_str(),
        &known,
        typeset,
    )?;
    Ok((todo, event, Some(revision)))
}

/// Why this priority when the optional line is blank: where the number came from.
pub(crate) const APP_RATIONALE: &str = "Set by hand in the app.";
/// An action line, not a document.
pub const MAX_BODY_CHARS: usize = 2000;
/// A sentence.
pub(crate) const MAX_RATIONALE_CHARS: usize = 500;
/// A screen and a half, generously.
pub(crate) const MAX_DETAIL_CHARS: usize = 8000;
/// A sentence.
pub(crate) const MAX_WAITING_ON_CHARS: usize = 500;
/// A checklist that long is a mistake, not a todo.
pub(crate) const MAX_SUBTODOS: usize = 200;

/// A change's prose within the caps a new todo's has.
fn capped_patch(patch: &suzatary_record::todo::TodoPatch) -> Result<(), Refusal> {
    for (name, text, cap) in [
        ("body", &patch.body, MAX_BODY_CHARS),
        ("detail", &patch.detail, MAX_DETAIL_CHARS),
        ("detail_append", &patch.detail_append, MAX_DETAIL_CHARS),
        ("rationale", &patch.rationale, MAX_RATIONALE_CHARS),
        ("waiting_on", &patch.waiting_on, MAX_WAITING_ON_CHARS),
    ] {
        if let Some(text) = text.set() {
            capped(name, text, cap)?;
        }
    }
    if let Some(subtodos) = patch.subtodos.set() {
        if subtodos.len() > MAX_SUBTODOS {
            return Err(Refusal::bad_request(format!(
                "subtodos is capped at {MAX_SUBTODOS} lines, got {}",
                subtodos.len()
            )));
        }
        for sub in subtodos {
            capped("subtodos[].body", &sub.body, MAX_BODY_CHARS)?;
        }
    }
    Ok(())
}

fn capped(name: &str, text: &str, cap: usize) -> Result<(), Refusal> {
    let count = text.chars().count();
    if count > cap {
        return Err(Refusal::bad_request(format!(
            "{name} is capped at {cap} characters, got {count}"
        )));
    }
    Ok(())
}

/// A todo id off the wire, checked before it touches a path: `..` is a perfectly good string.
fn todo_id(value: &str) -> Result<&str, Refusal> {
    static TODO_ID: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\A[a-z0-9_-]+\z").expect("a literal pattern")
    });
    if TODO_ID.is_match(value) {
        Ok(value)
    } else {
        Err(Refusal::bad_request(format!(
            "a todo id is [a-z0-9_-]+, got {}",
            shown(value)
        )))
    }
}

/// `GET /api/overview`: what is connected, by group, and what the box's registers say of it.
#[must_use]
pub fn overview(
    declared: &suzatary_core::settings::overview::Declared<'_>,
    asked: &suzatary_core::settings::overview::Asked,
    registers: suzatary_core::settings::overview::Registers<'_>,
    asking: &[suzatary_core::settings::overview::Asking],
) -> Overview {
    suzatary_core::settings::overview::overview(declared, &asked.observed(registers)).wire(
        &declared.settables.names(),
        asking,
        suzatary_base::conversation_id::SETUP.as_str().to_owned(),
        suzatary_core::connect::offered(declared.channels)
            .into_iter()
            .map(|template| Offer {
                template: template.id.as_str().to_owned(),
                group: template.group,
                title: template.title.to_owned(),
            })
            .collect(),
    )
}

/// `GET /api/calendar/<date>`: a week of the owner's calendars, as Typst's HTML and plain text.
pub fn calendar(
    calendar: &dyn suzatary_core::calendar::effect::Calendar,
    render: &dyn Render,
    from: NaiveDate,
) -> Answered<wire::CalendarPayload> {
    const DAYS: u32 = 7;
    let days = calendar
        .days(from, DAYS)
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    Ok(wire::CalendarPayload {
        from: from.to_string(),
        days: DAYS,
        text: suzatary_core::calendar::page::calendar_text(&days),
        rich: suzatary_core::rendering::CALENDAR
            .call(&suzatary_core::calendar::page::calendar(&days))
            .ok()
            .and_then(|page| render.page(&page).ok())
            .map(|html| html.as_str().to_owned()),
    })
}

/// Every reading of the lines a prune must leave as it was, each printed: every conversation as
/// the box reads it, the lines' whole skeleton read from nothing with books of its own at `at`,
/// and every line's content answer, asked as a device asks ([`asks`]).
pub fn lines_readings(
    conversations: suzatary_core::secretary::conversations::Conversations<'_>,
    at: prodrome_core::literal::Datetime,
) -> Answered<[(&'static str, String); 3]> {
    let lines = suzatary_core::secretary::conversations::list(conversations)?;
    let (epochs, frontiers) = (
        suzatary_signatures::epoch::Epochs::remembered(),
        prodrome_core::store::frontier::Frontiers::remembered(prodrome_core::store::frontier::KEPT),
    );
    let skeleton = lines_skeleton(
        conversations,
        &prodrome_core::store::frontier::Since::Nothing,
        Books {
            epochs: &epochs,
            frontiers: &frontiers,
        },
        at,
        Cut::Whole,
    )?;
    let mut content = Vec::new();
    let nodes = skeleton
        .nodes
        .iter()
        .map(|node| (node.hash.as_str(), node.size));
    for names in asks(nodes) {
        let hashes = crate::query::Hashes::parse(Some(&names.join(",")))?;
        content.push(lines_content(conversations, &hashes)?);
    }
    let json = |value: Result<String, serde_json::Error>| {
        value.map_err(|failed| Refusal::internal(failed.to_string()))
    };
    Ok([
        ("lines", format!("{lines:?}")),
        ("skeleton", json(serde_json::to_string(&skeleton))?),
        ("content", json(serde_json::to_string(&content))?),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::secrets::SecretState;

    /// A value the fake's store reads as locked.
    const LOCKED: &str = "<the store is locked>";

    /// A fake `Secrets` holding real-shaped values, so a leak would show.
    struct FakeSecrets {
        values: BTreeMap<String, String>,
        grants: BTreeMap<String, suzatary_core::secrets::MailGrants>,
    }

    impl suzatary_core::secrets::Secrets for FakeSecrets {
        fn secret_state(&self, settable: &suzatary_core::settings::Settable) -> SecretState {
            match self.values.get(settable.name()).map(String::as_str) {
                Some(LOCKED) => SecretState::Locked,
                Some(_) => SecretState::Set,
                None => SecretState::Missing,
            }
        }

        fn mail_grants(
            &self,
            account: &suzatary_core::mail::account::MailAccount,
        ) -> suzatary_core::secrets::MailGrants {
            self.grants
                .get(account.email.as_str())
                .copied()
                .unwrap_or_default()
        }

        fn google_grant(
            &self,
            _: &suzatary_core::mail::address::EmailAddress,
        ) -> suzatary_core::google::Held {
            suzatary_core::google::Held::NotSignedIn
        }
    }

    #[test]
    fn a_message_id_excludes_what_notmuchs_query_language_reads_as_syntax() {
        assert!(check_mail_id("CAB+abc123@mail.example.test").is_ok());
        assert!(check_mail_id("a1b2c3d4e5f6").is_ok());
        for bad in [
            "ab",
            "has space",
            "has\"quote",
            "has(paren",
            "star*",
            "colon:here",
        ] {
            assert!(check_mail_id(bad).is_err(), "{bad} should be refused");
        }
        assert!(check_mail_id(&"x".repeat(321)).is_err());
    }

    #[test]
    fn a_conversation_id_is_lowercase_hexadecimal() {
        assert!(check_thread_id("0000000012ab34cd").is_ok());
        for bad in ["", "abc", "ABCDEF01", "zzzz", &"a".repeat(33)] {
            assert!(check_thread_id(bad).is_err(), "{bad} should be refused");
        }
        assert_eq!(
            check_thread_id("nope").unwrap_err().message(),
            "a conversation id is hexadecimal, got 'nope'"
        );
    }

    /// Counts the lookups it does not answer.
    struct Counting<'a>(
        &'a dyn suzatary_core::secrets::Secrets,
        std::sync::atomic::AtomicUsize,
    );

    impl suzatary_core::secrets::Secrets for Counting<'_> {
        fn secret_state(&self, settable: &suzatary_core::settings::Settable) -> SecretState {
            let state = self.0.secret_state(settable);
            if matches!(state, SecretState::Locked | SecretState::Failed) {
                self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            state
        }

        fn mail_grants(
            &self,
            account: &suzatary_core::mail::account::MailAccount,
        ) -> suzatary_core::secrets::MailGrants {
            self.0.mail_grants(account)
        }

        fn google_grant(
            &self,
            account: &suzatary_core::mail::address::EmailAddress,
        ) -> suzatary_core::google::Held {
            self.0.google_grant(account)
        }
    }

    /// A value that looks leaked, so a leak would show.
    const LEAKED: &str = "sk-do-not-print-this-key";

    /// What the overview's tests declare.
    struct Instance {
        deployment: Deployment,
        settables: suzatary_core::settings::Settables,
        channels: Vec<suzatary_core::connect::Channel>,
        accounts: suzatary_core::mail::account::Roster,
        jobs: Vec<suzatary_core::settings::overview::ModelJob>,
        plugins: Vec<suzatary_core::plugin::Manifest>,
        build: suzatary_core::settings::overview::Build,
    }

    impl Instance {
        fn new() -> Self {
            let channels = suzatary_core::connect::Channel::ALL.to_vec();
            let plugins = vec![suzatary_core::settings::overview::calendar()];
            let account = |email: &str, sync| suzatary_core::mail::account::MailAccount {
                email: suzatary_core::mail::account_email::AccountEmail::new(email)
                    .expect("a lower-case address"),
                subdir: email.replace(['@', '.'], "-"),
                sync,
            };
            let ms =
                suzatary_core::mail::account::Sync::MsImap(suzatary_core::mail::account::MsImap {
                    authority: suzatary_core::mail::account::Authority::Organizations,
                    inbox_folders: suzatary_core::mail::folders::Folders::inbox(),
                    sent_folder: "Sent Items".to_owned(),
                });
            let chain = ChainPolicy {
                name: suzatary_base::chains::ChainName::new("life").expect("a slug"),
                untrusted: Untrusted::of([
                    prodrome_core::event::Actor::new("triage").expect("an actor")
                ]),
                roster: ["ada", "triage"]
                    .map(|name| prodrome_core::event::Actor::new(name).expect("an actor"))
                    .into_iter()
                    .collect(),
                root: suzatary_core::cast::keys::root(),
            };
            Self {
                deployment: Deployment {
                    viewers: std::collections::BTreeSet::new(),
                    actors: BTreeMap::new(),
                    chains: suzatary_base::chains::Chains::of(vec![chain]).expect("one chain"),
                    categories: BTreeMap::new(),
                    app_url: "https://example.test".to_owned(),
                    coding: None,
                    zone: suzatary_base::zone::Zone::UTC,
                },
                settables: suzatary_core::settings::Settables::derive(
                    suzatary_core::settings::MODEL_TOKEN,
                    channels.iter().copied(),
                    None,
                    &plugins,
                ),
                accounts: suzatary_core::mail::account::Roster::new(vec![
                    account(
                        "ada@example.test",
                        suzatary_core::mail::account::Sync::Gmail,
                    ),
                    account("ada@university.example", ms),
                ])
                .expect("a roster"),
                jobs: vec![suzatary_core::settings::overview::ModelJob {
                    label: "Chat".to_owned(),
                    detail: "Chat, reviews and titles run a-model.".to_owned(),
                    calls: suzatary_core::settings::overview::Calls::Recorded {
                        key: "a-cli/a-model".to_owned(),
                        credential: suzatary_core::settings::MODEL_TOKEN.to_owned(),
                    },
                }],
                channels,
                plugins,
                build: suzatary_core::settings::overview::Build {
                    version: "dev".to_owned(),
                    revision: "dev".to_owned(),
                    completed: Vec::new(),
                },
            }
        }

        fn overview(&self, secrets: &dyn suzatary_core::secrets::Secrets) -> Overview {
            let declared = suzatary_core::settings::overview::Declared {
                deployment: &self.deployment,
                settables: &self.settables,
                channels: &self.channels,
                accounts: self.accounts.accounts(),
                jobs: &self.jobs,
                plugins: &self.plugins,
                drive: &[],
                storage: &suzatary_core::settings::overview::Storage {
                    root: "/srv/data".to_owned(),
                    pushed: suzatary_core::settings::remote::Remote::parse(
                        "git@example.test:ada/data.git",
                    )
                    .map_or(
                        suzatary_core::settings::remote::Pushed::Nowhere,
                        suzatary_core::settings::remote::Pushed::To,
                    ),
                },
                build: &self.build,
                reach: &suzatary_core::connect::PLAIN,
            };
            let calendar = BTreeMap::from([(
                suzatary_core::health::plugin::PluginName::parse("ics").expect("a name"),
                suzatary_core::settings::overview::Store::Calendar,
            )]);
            overview(
                &declared,
                &suzatary_core::settings::overview::Asked::of(
                    secrets,
                    &self.settables,
                    self.accounts.accounts(),
                ),
                suzatary_core::settings::overview::Registers {
                    health: &suzatary_core::health::Healths::default(),
                    answered: &suzatary_core::health::Healths::default(),
                    plugins: &calendar,
                    pushes: None,
                },
                &[],
            )
        }
    }

    fn holding(names: &[&str]) -> FakeSecrets {
        FakeSecrets {
            values: names
                .iter()
                .map(|name| ((*name).to_owned(), LEAKED.to_owned()))
                .collect(),
            grants: BTreeMap::new(),
        }
    }

    fn group(
        page: &Overview,
        which: suzatary_core::connect::Group,
    ) -> &[suzatary_core::wire::cards::OverviewLine] {
        &page
            .groups
            .iter()
            .find(|group| group.group == which)
            .expect("every group is listed")
            .lines
    }

    /// A store that cannot answer is waited on once however many names are declared, and a line
    /// it reaches reads as unknown, never as missing.
    #[test]
    fn a_locked_store_is_asked_once_and_never_reads_as_unset() {
        let instance = Instance::new();
        let locked = FakeSecrets {
            values: instance
                .settables
                .names()
                .into_iter()
                .map(|name| (name.to_owned(), LOCKED.to_owned()))
                .collect(),
            grants: BTreeMap::new(),
        };
        let counting = Counting(&locked, std::sync::atomic::AtomicUsize::new(0));
        let page = instance.overview(&counting);
        assert_eq!(counting.1.load(std::sync::atomic::Ordering::SeqCst), 1);
        let models = group(&page, suzatary_core::connect::Group::Models);
        let chat = models
            .iter()
            .find(|line| line.key == "job:Chat")
            .expect("the model's line");
        assert_eq!(chat.standing, suzatary_core::connect::Standing::Locked);
    }

    /// No resolved value, and no secret's stored name, is drawn: names travel only where a card
    /// is matched.
    #[test]
    fn the_overview_never_carries_a_secret_value_and_draws_no_name() {
        let instance = Instance::new();
        let names = instance.settables.names();
        let page = instance.overview(&holding(&names));
        let json = serde_json::to_string(&page).expect("an overview serialises");
        assert!(!json.contains(LEAKED), "a value leaked: {json}");
        for name in &names {
            assert!(!json.contains(name), "{name} is on the wire: {json}");
        }
    }
}
