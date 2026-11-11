//! The routes as functions: each takes the capabilities it needs and the URL's parameters and
//! answers a wire type or a [`Refusal`], with nothing known about HTTP.
//!
//! The bounds are the point: a route that cannot name a capability cannot use it.

use crate::secretary::conversations;
use std::collections::BTreeMap;

use crate::believed::{Believed, records};
use crate::record::{Node, TodoEvent};
use chrono::NaiveDate;
use prodrome_core::fpl;
use prodrome_core::observe::Observation;
use prodrome_core::policy::Untrusted;

use crate::chain_set::ChainSet;

use crate::capability::{Clock, Datetime};
use crate::chains::{ChainName, ChainPolicy};
use crate::deployment::Deployment;
use crate::mail::mailbox::{Mailbox, Query};
use crate::refusal::{Answered, Refusal, shown};
use crate::render::Render;
use crate::secretary::answers::Answers;
use crate::secretary::briefings::Briefings;
use crate::secretary::step::Step;
use crate::store::Prodrome;
use crate::tree::{By, Classifier, Monoid};
use crate::wire::cards::{AboutPayload, HeldTodo, Offer, Overview, Shipped};
use crate::wire::lines::{
    Content, Frontier, Heads, NodeHeader, ObjectsPayload, ReceiptPayload, RefusedObject,
    SkeletonNode, SkeletonPayload, StoredObject,
};
use crate::wire::store::SinceRead;
use crate::wire::todo::{Entry, EntryState, Fulfillment, Neighbour};
use crate::{mail, query, reading};
use crate::{view, wire};

/// Every head, sorted: what a replica compares against and asks `since=` by, with no one of
/// them picked, since no writer needs a single head.
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

/// Every object in linearisation order with its hash, parents and canonical literal, so the
/// browser rehashes rather than believes.
///
/// `untrusted` is the deployment's trust policy, sent because the browser folds these objects
/// offline with the same core.
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

/// Every todo the chain has mentioned, as the folds see it at `t`; `limit` selects the most
/// owed, ascending.
///
/// It reads the chain's name and trust policy alone, so the tab, which holds no root key, answers
/// it from its replica as the box does.
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

/// The chain `name` as the set read it: its objects by name, its nodes and its rows, each priced through
/// the whole set.
#[allow(clippy::type_complexity)]
fn rows<'s>(
    set: &'s ChainSet<'_>,
    name: &ChainName,
) -> Answered<(
    std::collections::BTreeMap<prodrome_core::event::Hash, &'s crate::record::Envelope>,
    &'s [crate::record::Node],
    Vec<crate::record::Entry>,
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
fn not_a_member(failure: crate::chain_set::NotAMember) -> Refusal {
    match failure {
        crate::chain_set::NotAMember::Absent(_) => Refusal::not_found(failure.to_string()),
        other => Refusal::internal(other.to_string()),
    }
}

/// The `n` open todos most in need of attention, ascending in the fold's own `value`; ties
/// keep the fold's order.
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

/// One todo in full, with the routine "Build it" fires for it ([`Deployment::builder`]); an id no
/// event mentions is a 404, not an empty page.
pub fn todo(
    render: &dyn Render,
    chain: &ChainPolicy,
    zone: crate::zone::Zone,
    set: &ChainSet<'_>,
    id: &str,
    builder: Option<&crate::connect::RoutineId>,
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

/// Every day the assistant has saved a briefing for, newest first.
pub fn briefings(
    briefings: &dyn Briefings,
    conversations: crate::secretary::conversations::Conversations<'_>,
) -> Answered<wire::BriefingsPayload> {
    let saved = briefings
        .all()
        .map_err(|e| Refusal::internal(e.to_string()))?;
    let mut briefs: Vec<crate::secretary::briefs::Brief> = conversations::list(conversations)?
        .iter()
        .flat_map(crate::secretary::briefs::briefs)
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
                title: crate::secretary::briefs::headline(&brief.text),
                words: u32::try_from(crate::secretary::briefs::words(&brief.text))
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

/// The first `= …` heading's text, or the date when the file has none, without a compile.
fn briefing_title(source: &str, day: NaiveDate) -> String {
    source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('='))
        .map(|line| line.trim_start_matches('=').trim())
        .find(|text| !text.is_empty())
        .map_or_else(|| day.to_string(), str::to_owned)
}

/// One day's briefing: that day's secretary brief rendered block by block, else the saved
/// file, which renders unconditionally because `briefings/` is trusted by convention.
pub fn briefing(
    briefings: &dyn Briefings,
    render: &dyn Render,
    conversations: crate::secretary::conversations::Conversations<'_>,
    answers: &dyn Answers,
    day: NaiveDate,
) -> Answered<wire::BriefingPayload> {
    let brief = conversations::list(conversations)?
        .iter()
        .flat_map(crate::secretary::briefs::briefs)
        .filter(|brief| brief.date == day)
        .max_by_key(|brief| brief.turn);
    if let Some(brief) = brief {
        return Ok(wire::BriefingPayload {
            date: day.to_string(),
            day: wire::BriefingDay::Brief {
                conversation: brief.conversation.as_str().to_owned(),
                turn: u32::try_from(brief.turn).unwrap_or(u32::MAX),
                blocks: crate::secretary::readings::blocks_of(answers, &brief.text),
                text: brief.text,
            },
        });
    }
    let source = briefings
        .read(day)
        .map_err(|_| Refusal::not_found(format!("no briefing for {day}")))?;
    let markup = crate::escape::Escaped::from_chain(source.clone());
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

/// The account filter resolved against the roster; an unknown address is a refusal, never an
/// empty inbox.
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

/// The most recent inbox mail by date, not the agent's queue, with its disposition as a badge.
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

/// A message id: a notmuch locator or a canonical hash prefix, excluding notmuch query syntax
/// because it is interpolated into a query.
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

/// One message: headers, body text, links, conversation, and the todos it produced.
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
        Err(crate::mail_error::MailError::Absent { .. }) => {
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

/// What a message no longer kept reads as: the words, with the subject and sender a todo
/// citing it recorded.
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
    Ok(crate::mail::gone::Gone {
        subject: source.map(|source| source.subject.as_str()),
        sender: source.map(|source| source.sender.as_str()),
    }
    .said())
}

/// One conversation, oldest first; 404 when the corpus knows no such thread.
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
        crate::mail_error::MailError::Absent { .. } => Refusal::not_found(format!(
            "{}: conversation {}",
            crate::mail::gone::SAID,
            shown(id)
        )),
        other => Refusal::internal(other.to_string()),
    })?;
    if found.is_empty() {
        return Err(Refusal::not_found(format!(
            "{}: conversation {}",
            crate::mail::gone::SAID,
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

/// Every todo whose latest `Authored` names this message, one-directional since the chain
/// stores a hash prefix.
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

/// Every confirmed markup fragment on the chain worth compiling at `now`.
pub fn warmable(
    store: &dyn Prodrome<TodoEvent>,
    untrusted: &Untrusted,
    now: Datetime,
) -> Answered<Vec<crate::escape::Escaped>> {
    let records = records(&store.nodes()?, now);
    Ok(view::renderable(records.values().flatten(), untrusted))
}

/// The clock's now, as every route's default `t`.
pub fn now(clock: &dyn Clock) -> Datetime {
    clock.now()
}

/// What every pull records beside its answer: each store's epoch, and the frontiers it answered.
#[derive(Clone, Copy)]
pub struct Books<'a> {
    /// Each store's epoch.
    pub epochs: &'a crate::epoch::Epochs,
    /// Each store's recorded frontiers.
    pub frontiers: &'a prodrome_core::store::frontier::Frontiers,
}

/// `GET /api/objects?since=<frontier>`: the objects the frontier `since` names does not reach,
/// all of them for a frontier this box does not hold.
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
    /// All of it, as a chain's pull answers.
    Whole,
    /// The longest run of it, parents first, whose literals fit this many bytes, and never
    /// less than one object: a piece of a store's pull, which a tab takes in and keeps before it
    /// asks for the next from its new heads.
    Bytes(usize),
}

impl Cut {
    /// A piece of a store's pull: a megabyte of literals, about six seconds at 1.5 Mbps, well
    /// under the thirty a tab waits for a read (`web/src/wire/http.ts`).
    pub const PIECE: Cut = Cut::Bytes(1 << 20);
}

/// `GET /api/stores/<store>/objects?since=<frontier>`: what a replica of `store` lacks, cut as
/// `cut` says, parents first, with every head and how many objects are left. The pieces asked for
/// one after another, each from the frontier the last named, are together one answer of the whole.
///
/// # Errors
/// The store's own, and the books'.
pub fn store_objects_since<E: crate::stores::Store>(
    store: &dyn Prodrome<E>,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
    cut: Cut,
) -> Answered<wire::StorePayload> {
    store_answer(store, since, books, now, cut).map(|(payload, _, _)| payload)
}

/// [`store_objects_since`], with the names of the objects it sends and the graph it read them
/// from.
fn store_answer<E: crate::stores::Store>(
    store: &dyn Prodrome<E>,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
    cut: Cut,
) -> Answered<(
    wire::StorePayload,
    Vec<prodrome_core::event::Hash>,
    std::sync::Arc<crate::store::Graph<E>>,
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

/// One object as a replica carries it, weighed by its literal's bytes.
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

/// What a store held at one answer, in the epoch it was read in, how it read `since`, and the
/// frontier the asker holds once it has taken the answer.
struct Snapshot {
    heads: Vec<String>,
    geneses: Vec<String>,
    epoch: u64,
    since: SinceRead,
    frontier: Frontier,
}

impl Snapshot {
    /// An objects answer of these objects, read under no policy: the caller's.
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

/// What one answer of a pull sends: the items in linearisation order, how many it names are left
/// out, the weight of those sent, and the weight of all it names, sent or not.
struct Piece<T> {
    sent: Vec<(prodrome_core::event::Hash, T)>,
    rest: usize,
    weight: usize,
    size: usize,
}

impl<T> Piece<T> {
    /// What a replica lacking nothing is sent.
    fn none() -> Self {
        Self {
            sent: Vec::new(),
            rest: 0,
            weight: 0,
            size: 0,
        }
    }

    /// The items sent, without their names.
    fn items(self) -> Vec<T> {
        self.sent.into_iter().map(|(_, item)| item).collect()
    }
}

/// What a pull sends of each object it answers: the object with its literal, or a skeleton's
/// node. An object past a piece's cut is only weighed, for the pull's whole size.
trait Sends<E: prodrome_core::schema::Schema> {
    /// One object as the answer carries it.
    type Item;
    /// Under which the store's graph keeps every object's weight, read once per head set.
    const WEIGHTS: &'static str;
    /// The object as sent, and its weight.
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
}

/// Each object with its literal, weighed by the literal's bytes.
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

/// Each line as its skeleton node, weighed by its bytes on the wire.
struct Nodes;

impl Sends<crate::secretary::line_event::LineEvent> for Nodes {
    type Item = SkeletonNode;
    const WEIGHTS: &'static str = "weights of nodes";

    fn emit(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<crate::secretary::line_event::LineEvent>,
    ) -> (SkeletonNode, usize) {
        // Sized by its file once cut ([`held_sizes`]), so its literal is not printed here.
        let node = node_of(name, envelope, 0);
        let weight = weight(&node);
        (node, weight)
    }

    fn weigh(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<crate::secretary::line_event::LineEvent>,
    ) -> usize {
        weight(&node_of(name, envelope, 0))
    }
}

/// An answer of `store`, read under its epoch's lock so the epoch and the objects are one
/// snapshot: every head, the prodromes it serves, and what `sends` makes of each object the
/// frontier `since` names does not reach, cut as `cut` says by the weight it gives each, in
/// linearisation order so a receiver can apply them as they arrive. The frontier the asker holds
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
        let seen = crate::epoch::Seen {
            geneses: names(graph.geneses()).into_iter().collect(),
            heads: names(graph.heads()).into_iter().collect(),
            held: std::sync::Arc::clone(graph.names()),
        };
        let lacking = books.frontiers.answer(key, since, graph.dag())?;
        let read = since_read(lacking.reached());
        let mut piece = Piece::none();
        let mut full = false;
        // Every piece of a pull weighs all it leaves out; read once per head set, a pull of many
        // pieces weighs each object once rather than once a piece.
        let weights = graph.reading(S::WEIGHTS, |graph| {
            graph
                .objects()
                .map(|(name, envelope)| (name.clone(), sends.weigh(name, envelope)))
                .collect::<std::collections::HashMap<_, _>>()
        });
        for &(name, envelope) in lacking.lacking() {
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
        let frontier = frontier_of(&lacking.record(piece.sent.len())?);
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

/// One answer of a pull, and the graph it was read from.
struct Pulled<E: prodrome_core::schema::Schema, T> {
    snapshot: Snapshot,
    piece: Piece<T>,
    graph: std::sync::Arc<crate::store::Graph<E>>,
}

/// Whether the store of record knew the frontier a pull named.
fn since_read(reached: &prodrome_core::store::frontier::Reached) -> SinceRead {
    match reached {
        prodrome_core::store::frontier::Reached::Everything
        | prodrome_core::store::frontier::Reached::Known(_) => SinceRead::Known,
        prodrome_core::store::frontier::Reached::Unknown => SinceRead::Unknown,
    }
}

/// The frontier an answer recorded, as the wire names it.
fn frontier_of(recorded: &prodrome_core::store::frontier::Recorded) -> Frontier {
    Frontier {
        digest: recorded.token.as_str().to_owned(),
        count: recorded.count,
    }
}

/// `GET /api/stores/lines/skeleton?since=<heads>`: the lines' pull, each object as its node,
/// sized by what [`lines_content`] sends of it, with every key its nodes name but a retired one.
///
/// # Errors
/// The stores' own.
pub fn lines_skeleton(
    conversations: crate::secretary::conversations::Conversations<'_>,
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
    // A replica that lacks nothing holds the store's heads already: they are the heads of what it
    // holds, which a recorded frontier names exactly.
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
fn held_sizes(
    lines: &dyn Prodrome<crate::secretary::line_event::LineEvent>,
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

/// What the lines of `graph` retire of their keys, kept in the graph.
fn retired(
    graph: &crate::store::Graph<crate::secretary::line_event::LineEvent>,
) -> std::sync::Arc<crate::secretary::transcript::Retired> {
    graph.reading("retired", |graph| {
        crate::secretary::transcript::Retired::of(graph.objects())
    })
}

/// The key of every slot the lines named `sent` name, by name, but a key `retired` says is dead,
/// erased or superseded by a reseal the box opens.
fn keys_of(
    conversations: crate::secretary::conversations::Conversations<'_>,
    graph: &crate::store::Graph<crate::secretary::line_event::LineEvent>,
    retired: &crate::secretary::transcript::Retired,
    sent: &[prodrome_core::event::Hash],
) -> Answered<BTreeMap<String, String>> {
    let reads = conversations.reads();
    let slots = |name: &crate::secretary::locked_text::KeyName| reads.key(name);
    let line = |name: &prodrome_core::event::Hash| graph.dag().get(name)?.event();
    let mut keys = BTreeMap::new();
    for name in sent
        .iter()
        .filter_map(line)
        .flat_map(|sent| sent.keys().map(move |key| (sent.conversation(), key)))
        .filter(|(id, name)| !retired.retires(id, name, line, &slots))
        .map(|(_, name)| name)
    {
        if let Some(key) = conversations.keys.get(name)? {
            keys.insert(
                name.as_str().to_owned(),
                crate::secretary::key_ring::spelled(&key),
            );
        }
    }
    Ok(keys)
}

/// One line's node: its name, what it rests on, its literal's size, and its header.
#[must_use]
pub fn skeleton_node(
    name: &prodrome_core::event::Hash,
    envelope: &prodrome_core::event::Envelope<crate::secretary::line_event::LineEvent>,
) -> SkeletonNode {
    node_of(
        name,
        envelope,
        prodrome_core::event::canonical_envelope(envelope).len(),
    )
}

/// One line's node, sized `size`.
fn node_of(
    name: &prodrome_core::event::Hash,
    envelope: &prodrome_core::event::Envelope<crate::secretary::line_event::LineEvent>,
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

/// A line's header, what a fold reads of it without its words: its husk's is its own.
#[must_use]
pub fn line_header(line: &crate::secretary::line_event::LineEvent) -> NodeHeader {
    NodeHeader::Line {
        conversation: line.conversation().as_str().to_owned(),
        at: fpl::iso(fpl::instant_of(line.at())),
        kind: line.shape().name().to_owned(),
        keys: line.keys().map(|key| key.as_str().to_owned()).collect(),
        step: Step::of(line.shape()),
    }
}

/// An envelope's constructor, as its literal spells it.
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

/// About how many bytes a node is on the wire: its strings and a little for the JSON around
/// them, which is what a skeleton's piece is cut by.
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

/// The most bytes of literals one content answer of more than one hash carries: a piece's
/// ([`Cut::PIECE`]). A single object is answered whatever its size, since a line written before
/// a stored answer was bounded can be larger.
pub const MOST_CONTENT_BYTES: usize = 1 << 20;

/// The content questions for `nodes` (`(name, size)`), ascending, each within the bounds an answer
/// carries, an object past them asked alone.
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

/// `GET /api/stores/<store>/content?hashes=` and a chain's `…/content?hashes=`: the literal of
/// each object `hashes` names, in that order, from `store` alone. A hash the store does not hold,
/// or literals past [`MOST_CONTENT_BYTES`], refuse the whole question in the same words.
///
/// # Errors
/// [`Refusal::not_found`] naming the hashes the store does not hold;
/// [`Refusal::bad_request`] for literals past the bound; the store's own.
pub fn content<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
    hashes: &query::Hashes,
) -> Answered<wire::ContentPayload> {
    answer(store, &*store.graph()?, hashes, &BTreeMap::new())
}

/// [`content`] of the lines, each line every key of whose words the box retires as its husk. A
/// line with one live slot and one dead slot is sent whole, so the dead slot's sealed bytes go
/// with it: a husk of some slots alone could be checked against nothing.
pub fn lines_content(
    conversations: crate::secretary::conversations::Conversations<'_>,
    hashes: &query::Hashes,
) -> Answered<wire::ContentPayload> {
    let graph = conversations.lines.graph()?;
    let names: Vec<prodrome_core::event::Hash> = hashes.iter().cloned().collect();
    let printed = husks(conversations, &graph, &names)?;
    answer(conversations.lines, &graph, hashes, &printed)
}

/// The printed husk of each line of `names` with words no key the box sends opens.
pub fn husks(
    conversations: crate::secretary::conversations::Conversations<'_>,
    graph: &crate::store::Graph<crate::secretary::line_event::LineEvent>,
    names: &[prodrome_core::event::Hash],
) -> Answered<BTreeMap<prodrome_core::event::Hash, String>> {
    let sent = keys_of(conversations, graph, &retired(graph), names)?;
    Ok(husked(graph, &sent, names))
}

/// [`husks`] of `names`, given `sent`, the keys [`keys_of`] sends of them.
fn husked(
    graph: &crate::store::Graph<crate::secretary::line_event::LineEvent>,
    sent: &BTreeMap<String, String>,
    names: &[prodrome_core::event::Hash],
) -> BTreeMap<prodrome_core::event::Hash, String> {
    names
        .iter()
        .filter_map(|name| {
            let line = graph.dag().get(name)?.event()?;
            if line.keys().any(|key| sent.contains_key(key.as_str())) {
                return None;
            }
            Some((name.clone(), line.husk()?.print()))
        })
        .collect()
}

/// Each object `hashes` names that `graph`, the store's history read once for this question,
/// holds: its husk where `husks` holds one, else its literal read by its name.
fn answer<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
    graph: &crate::store::Graph<E>,
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

/// `GET /api/waiting`: the badge's count of the Inbox's cards (open proposals, open decisions on
/// `chains`, and the proposal lines no inbox record places, which it names by conversation),
/// the lines read through the box's one memoised fold, so a call costs what moved since the last.
///
/// # Errors
/// A store that does not read, a chain the set does not hold, and lines the fold refuses: no
/// count is better than a wrong one.
pub fn waiting(
    inbox: &crate::secretary::inbox::Fold,
    conversations: crate::secretary::conversations::ReadConversations<'_>,
    set: &ChainSet<'_>,
    chains: &[ChainName],
) -> Answered<wire::WaitingPayload> {
    let proposals = crate::secretary::readings::waiting(inbox, set).len();
    let mut decisions = 0usize;
    for chain in chains {
        let (by_hash, _, rows) = rows(set, chain)?;
        decisions += rows
            .iter()
            .filter(|row| {
                view::content_of(row, &by_hash).is_some_and(|record| {
                    record.payload.is_decision()
                        && EntryState::from(crate::believed::one_outcome(row.outcome()))
                            == EntryState::Open
                })
            })
            .count();
    }
    let mut unplaced = BTreeMap::new();
    for (id, read) in conversations.fold(crate::secretary::held_lines::HeldLines::transcripts)? {
        let read = read.map_err(|refused| Refusal::internal(refused.to_string()))?;
        let lines = crate::secretary::readings::unplaced(&read.stored().lines(inbox));
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
pub(crate) const MAX_POSTED: usize = 512;

/// `POST /api/objects`: the one write path, deciding under the write lock and then taking in
/// what the outcome names.
pub fn receive_objects(
    store: &dyn Prodrome<TodoEvent>,
    render: &dyn Render,
    chain: &ChainPolicy,
    posted: &Posted<'_>,
    frontiers: &prodrome_core::store::frontier::Frontiers,
    actor: &prodrome_core::event::Actor,
) -> Answered<(ReceiptPayload, Vec<crate::secretary::act_lines::ActLine>)> {
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
    let outcome = crate::sync::receive(
        &crate::sync::Objects::of(&objects),
        posted,
        actor,
        &chain.root,
    );
    let taken: Vec<TodoEvent> = outcome
        .taken
        .iter()
        .filter_map(|(_, literal)| {
            prodrome_core::event::parse_envelope(&crate::record::TodoVocabulary::default(), literal)
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
    let heard =
        crate::secretary::act_lines::heard_from_objects(&taken, actor, &chain.name, &|posted| {
            crate::todos::current(&shown, posted.at, posted.todo.as_str())
        });
    store.accept(&outcome.taken)?;
    // Render what landed before answering; `renders` keeps untrusted records uncompiled.
    let fresh: Vec<crate::escape::Escaped> = taken
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

/// What a post carries: the prints, and the frontier the poster held before them.
#[derive(Clone, Copy)]
pub struct Posted<'a> {
    /// Canonical prints, in any order.
    pub objects: &'a [String],
    /// The frontier the poster held, as its last answer named it.
    pub since: &'a prodrome_core::store::frontier::Since,
}

/// `POST /api/stores/<store>/objects`, for a store a tab writes that the box began and nobody
/// signs: decided under the write lock, then what the outcome names taken in, through the
/// store's own verify-on-receive.
///
/// # Errors
/// More than `MAX_POSTED` objects, or the store not read or not written.
pub fn receive_changes<E: crate::stores::Store>(
    store: &dyn Prodrome<E>,
    posted: &Posted<'_>,
    frontiers: &prodrome_core::store::frontier::Frontiers,
    actor: &prodrome_core::event::Actor,
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
    let _held = store.locked()?;
    let held = store.objects()?.into_iter().map(|(name, _)| name).collect();
    let outcome = crate::sync::receive_changes::<E>(&held, posted, actor);
    store.accept(&outcome.taken)?;
    let key = format!("store/{}", E::KIND.name());
    receipt(store, &outcome, (&key, since, frontiers))
}

/// What a post took, already held and refused, beside the store's heads after it, and the
/// frontier the poster holds now: what `since` named, if this box holds it, and every object it
/// took or already held, with all they rest on.
fn receipt<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
    outcome: &crate::sync::Outcome,
    (key, since, frontiers): (
        &str,
        &prodrome_core::store::frontier::Since,
        &prodrome_core::store::frontier::Frontiers,
    ),
) -> Answered<ReceiptPayload> {
    let objects = store.objects()?;
    let dag: prodrome_core::dag::Dag<E> = objects.into_iter().collect();
    let lacking = frontiers.answer(key, since, &dag)?;
    let read = since_read(lacking.reached());
    let posted = outcome
        .taken
        .iter()
        .map(|(name, _)| name)
        .chain(&outcome.already)
        .cloned();
    let frontier = frontier_of(&lacking.record_with(posted)?);
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

/// `GET /api/about`: the release this binary was built as and its build, so the page shows what
/// runs, and the todos that release completed, each as `plan`'s chain reads it in `set`; `dev`
/// for a build no release was stamped on.
///
/// # Errors
/// The plan's chain held and not read.
pub fn about(
    released: Option<&crate::release::Released>,
    build: &str,
    plan: &crate::release::Plan,
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

/// Each of `todos` that `plan`'s chain in `set` holds, by id, as its row and title now: how a
/// release's todos are shown, looked up on the chain rather than stored with the release.
/// `None` where the set does not hold that chain.
fn held_todos(
    plan: &crate::release::Plan,
    set: &ChainSet<'_>,
    todos: &[prodrome_core::event::TodoId],
) -> Answered<Option<std::collections::BTreeMap<String, HeldTodo>>> {
    let name = crate::chains::ChainName::new(plan.chain())
        .map_err(|e| Refusal::internal(e.to_string()))?;
    match set.held(&name) {
        Ok(_) => {}
        Err(crate::chain_set::NotAMember::Absent(_)) => return Ok(None),
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
                    |record| crate::escape::markup_text(record.payload.body.as_str()),
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

/// `GET /api/releases`: `plan`'s releases as `set` reads them, each with its row, where it
/// stands in the count and its items; no release where the set does not hold the plan's chain.
///
/// # Errors
/// The chain held and not read.
pub fn releases(
    plan: &crate::release::Plan,
    set: &ChainSet<'_>,
) -> Answered<wire::ReleasesPayload> {
    let name = crate::chains::ChainName::new(plan.chain())
        .map_err(|e| Refusal::internal(e.to_string()))?;
    let held = match set.held(&name) {
        Ok(held) => held,
        Err(crate::chain_set::NotAMember::Absent(_)) => {
            return Ok(wire::ReleasesPayload {
                chain: plan.chain().to_owned(),
                reads: crate::release::Counted {
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
    let reading = crate::release::Reading::of(
        plan,
        crate::release::binding(held.nodes, held.untrusted),
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

/// `GET /api/<chain>/me`: the login the daemon named and the actor it writes as on this
/// chain, `null` for a viewer or an actor the chain's roster does not know.
#[must_use]
pub fn me(
    deployment: &Deployment,
    chain: &ChainPolicy,
    admitted: &crate::signatures::Admitted,
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

/// `POST /api/author`: build an event and append nothing, so the replica seals what the core
/// authored.
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
            let Some(before) = crate::todos::current(&nodes, at, &id) else {
                return Err(Refusal::not_found(format!(
                    "no todo with id {} on the chain",
                    shown(&id)
                )));
            };
            capped_patch(patch)?;
            let known: Vec<&str> = deployment.categories.keys().map(String::as_str).collect();
            let event = crate::authoring::update_todo(
                &before,
                patch,
                crate::zone::Moment {
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
            let Some(before) = crate::todos::current(&nodes, at, &id) else {
                return Err(Refusal::not_found(format!(
                    "the chain holds no content for todo {}",
                    shown(&id)
                )));
            };
            let event = crate::authoring::confirm_todo(
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
            let event = crate::authoring::undo(&stream, &id, at, actor.as_str(), &chain.untrusted)?;
            (id, event)
        }
        wire::Authoring::Redo(todo) => {
            let id = todo_id(todo)?.to_owned();
            let stream = view::stream_of(&nodes, &id);
            let event = crate::authoring::redo(&stream, &id, at, actor.as_str(), &chain.untrusted)?;
            (id, event)
        }
    };
    Ok(wire::AuthoredPayload {
        todo,
        event: prodrome_core::event::canonical(&event),
        revision: None,
    })
}

/// A new todo through the one authoring path, with its parent's revision when it is a subtodo.
///
/// The id is derived from the body unless one is given, since the body is the only handle a
/// person can reproduce from what they see, and never one any event has mentioned, a cancelled
/// todo's included.
fn add(
    deployment: &Deployment,
    chain: &ChainPolicy,
    nodes: &[Node],
    new: &wire::NewTodo,
    at: Datetime,
    actor: &prodrome_core::event::Actor,
    typeset: &dyn crate::capability::Typeset,
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
        None => crate::authoring::fresh_id(&crate::authoring::slug(&fields.body)?, &taken)?,
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
    let new_todo = crate::authoring::NewTodo {
        todo: todo.clone(),
        fields,
        source: None,
        subtodos: Vec::new(),
    };
    let Some(parent) = &new.parent else {
        let now = crate::zone::Moment {
            at,
            zone: deployment.zone,
        };
        let event = crate::authoring::build_todo(&new_todo, now, actor.as_str(), &known, typeset)?;
        return Ok((todo, event, None));
    };
    let Some(record) = crate::todos::current(nodes, at, todo_id(parent)?) else {
        return Err(Refusal::not_found(format!(
            "no todo with id {} on the chain to be a parent",
            shown(parent)
        )));
    };
    let believed =
        Believed::of(nodes, at, &chain.untrusted).map_err(|e| Refusal::internal(e.to_string()))?;
    let (event, revision) = crate::authoring::build_subtodo(
        &new_todo,
        &record,
        &believed.linkable(),
        crate::zone::Moment {
            at,
            zone: deployment.zone,
        },
        actor.as_str(),
        &known,
        typeset,
    )?;
    Ok((todo, event, Some(revision)))
}

/// Why this priority when the app's optional line is blank: where the number came from, which
/// is honest where an invented reason would not be.
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
fn capped_patch(patch: &crate::todo::TodoPatch) -> Result<(), Refusal> {
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

/// A todo id off the wire, checked before it touches a fold or a path: the compile cache is a
/// filesystem and `..` is a perfectly good string.
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

/// `GET /api/overview`: what is connected, by group, read from `declared`, what the secret
/// store answered (`asked`) and what the box's registers say of it now.
#[must_use]
pub fn overview(
    declared: &crate::settings::overview::Declared<'_>,
    asked: &crate::settings::overview::Asked,
    registers: crate::settings::overview::Registers<'_>,
    asking: &[crate::settings::overview::Asking],
) -> Overview {
    crate::settings::overview::overview(declared, &asked.observed(registers)).wire(
        &declared.settables.names(),
        asking,
        crate::conversation_id::SETUP.as_str().to_owned(),
        crate::connect::offered(declared.channels)
            .into_iter()
            .map(|template| Offer {
                template: template.id.as_str().to_owned(),
                group: template.group,
                title: template.title.to_owned(),
            })
            .collect(),
    )
}

/// `GET /api/calendar/<date>`: a week of the owner's calendars from that day, as Typst's HTML
/// and as plain text.
pub fn calendar(
    calendar: &dyn crate::calendar::effect::Calendar,
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
        text: crate::reading::calendar_text(&days),
        rich: crate::rendering::CALENDAR
            .call(&crate::reading::calendar(&days))
            .ok()
            .and_then(|page| render.page(&page).ok())
            .map(|html| html.as_str().to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::SecretState;

    /// A value the fake's store reads as locked: it holds the name and cannot say what it is.
    const LOCKED: &str = "<the store is locked>";

    /// A fake `Secrets` holding real-shaped values, so a leak would show.
    struct FakeSecrets {
        values: BTreeMap<String, String>,
        grants: BTreeMap<String, crate::secrets::MailGrants>,
    }

    impl crate::secrets::Secrets for FakeSecrets {
        fn secret_state(&self, settable: &crate::settings::Settable) -> SecretState {
            match self.values.get(settable.name()).map(String::as_str) {
                Some(LOCKED) => SecretState::Locked,
                Some(_) => SecretState::Set,
                None => SecretState::Missing,
            }
        }

        fn mail_grants(
            &self,
            account: &crate::mail::account::MailAccount,
        ) -> crate::secrets::MailGrants {
            self.grants
                .get(account.email.as_str())
                .copied()
                .unwrap_or_default()
        }

        fn google_grant(&self, _: &crate::mail::address::EmailAddress) -> crate::google::Held {
            crate::google::Held::NotSignedIn
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

    /// A store that counts the lookups it gives no answer to, each a wait for the timeout.
    struct Counting<'a>(
        &'a dyn crate::secrets::Secrets,
        std::sync::atomic::AtomicUsize,
    );

    impl crate::secrets::Secrets for Counting<'_> {
        fn secret_state(&self, settable: &crate::settings::Settable) -> SecretState {
            let state = self.0.secret_state(settable);
            if matches!(state, SecretState::Locked | SecretState::Failed) {
                self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            state
        }

        fn mail_grants(
            &self,
            account: &crate::mail::account::MailAccount,
        ) -> crate::secrets::MailGrants {
            self.0.mail_grants(account)
        }

        fn google_grant(
            &self,
            account: &crate::mail::address::EmailAddress,
        ) -> crate::google::Held {
            self.0.google_grant(account)
        }
    }

    /// A value that looks leaked, so a leak would show.
    const LEAKED: &str = "sk-do-not-print-this-key";

    /// What the overview's tests declare: the model, both channels, an account of each kind, a
    /// calendar of two links and one chain.
    struct Instance {
        deployment: Deployment,
        settables: crate::settings::Settables,
        channels: Vec<crate::connect::Channel>,
        accounts: crate::mail::account::Roster,
        jobs: Vec<crate::settings::overview::ModelJob>,
        plugins: Vec<crate::plugin::Manifest>,
        build: crate::settings::overview::Build,
    }

    impl Instance {
        fn new() -> Self {
            let channels = crate::connect::Channel::ALL.to_vec();
            let plugins = vec![crate::settings::overview::tests::calendar()];
            let account = |email: &str, sync| crate::mail::account::MailAccount {
                email: crate::mail::account_email::AccountEmail::new(email)
                    .expect("a lower-case address"),
                subdir: email.replace(['@', '.'], "-"),
                sync,
            };
            let ms = crate::mail::account::Sync::MsImap(crate::mail::account::MsImap {
                authority: crate::mail::account::Authority::Organizations,
                inbox_folders: crate::mail::folders::Folders::inbox(),
                sent_folder: "Sent Items".to_owned(),
            });
            let chain = ChainPolicy {
                name: crate::chains::ChainName::new("life").expect("a slug"),
                untrusted: Untrusted::of([
                    prodrome_core::event::Actor::new("triage").expect("an actor")
                ]),
                roster: ["ada", "triage"]
                    .map(|name| prodrome_core::event::Actor::new(name).expect("an actor"))
                    .into_iter()
                    .collect(),
                root: crate::cast::keys::root(),
            };
            Self {
                deployment: Deployment {
                    viewers: std::collections::BTreeSet::new(),
                    actors: BTreeMap::new(),
                    chains: crate::chains::Chains::of(vec![chain]).expect("one chain"),
                    categories: BTreeMap::new(),
                    app_url: "https://example.test".to_owned(),
                    coding: None,
                    zone: crate::zone::Zone::UTC,
                },
                settables: crate::settings::Settables::derive(
                    crate::settings::MODEL_TOKEN,
                    channels.iter().copied(),
                    None,
                    &plugins,
                ),
                accounts: crate::mail::account::Roster::new(vec![
                    account("ada@example.test", crate::mail::account::Sync::Gmail),
                    account("ada@university.example", ms),
                ])
                .expect("a roster"),
                jobs: vec![crate::settings::overview::ModelJob {
                    label: "Chat".to_owned(),
                    detail: "Chat, reviews and titles run a-model.".to_owned(),
                    calls: crate::settings::overview::Calls::Recorded {
                        key: "a-cli/a-model".to_owned(),
                        credential: crate::settings::MODEL_TOKEN.to_owned(),
                    },
                }],
                channels,
                plugins,
                build: crate::settings::overview::Build {
                    version: "dev".to_owned(),
                    revision: "dev".to_owned(),
                    completed: Vec::new(),
                },
            }
        }

        fn overview(&self, secrets: &dyn crate::secrets::Secrets) -> Overview {
            let declared = crate::settings::overview::Declared {
                deployment: &self.deployment,
                settables: &self.settables,
                channels: &self.channels,
                accounts: self.accounts.accounts(),
                jobs: &self.jobs,
                plugins: &self.plugins,
                drive: &[],
                storage: &crate::settings::overview::Storage {
                    root: "/srv/data".to_owned(),
                    pushed: crate::settings::remote::Remote::parse("git@example.test:ada/data.git")
                        .map_or(
                            crate::settings::remote::Pushed::Nowhere,
                            crate::settings::remote::Pushed::To,
                        ),
                },
                build: &self.build,
                reach: &crate::connect::PLAIN,
            };
            let calendar = BTreeMap::from([(
                crate::health::plugin::PluginName::parse("ics").expect("a name"),
                crate::settings::overview::Store::Calendar,
            )]);
            overview(
                &declared,
                &crate::settings::overview::Asked::of(
                    secrets,
                    &self.settables,
                    self.accounts.accounts(),
                ),
                crate::settings::overview::Registers {
                    health: &crate::health::Healths::default(),
                    answered: &crate::health::Healths::default(),
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

    fn group(page: &Overview, which: crate::connect::Group) -> &[crate::wire::cards::OverviewLine] {
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
        let models = group(&page, crate::connect::Group::Models);
        let chat = models
            .iter()
            .find(|line| line.key == "job:Chat")
            .expect("the model's line");
        assert_eq!(chat.standing, crate::connect::Standing::Locked);
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
