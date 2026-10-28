//! The routes as functions: each takes the capabilities it needs and the URL's parameters and
//! answers a wire type or a [`Refusal`], with nothing known about HTTP.
//!
//! The bounds are the point: a route that cannot name a capability cannot use it.

use std::collections::BTreeMap;

use crate::believed::{Believed, records};
use crate::record::{Node, TodoEvent};
use chrono::NaiveDate;
use prodrome_core::fpl;
use prodrome_core::observe::Observation;
use prodrome_core::policy::Untrusted;

use crate::chain_set::ChainSet;

use crate::capability::{Answers, Briefings, Clock, Datetime, Mailbox, Query, Render, SecretState};
use crate::chains::{ChainName, ChainPolicy};
use crate::deployment::Deployment;
use crate::refusal::{Answered, Refusal, shown};
use crate::store::Prodrome;
use crate::tree::{By, Classifier, Monoid};
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
    let entries: Vec<wire::Entry> = rows
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
fn most_owed(entries: Vec<wire::Entry>, n: usize) -> Vec<wire::Entry> {
    let mut owed: Vec<(f64, wire::Entry)> = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.state != wire::EntryState::Open {
            continue;
        }
        if let Some(value) = entry.value.and_then(wire::Fulfillment::number) {
            owed.push((value, entry));
        }
    }
    // `total_cmp`, so a NaN orders rather than panicking a read.
    owed.sort_by(|a, b| a.0.total_cmp(&b.0));
    owed.truncate(n);
    owed.into_iter().map(|(_, entry)| entry).collect()
}

/// One todo in full; an id no event mentions is a 404, not an empty page.
pub fn todo(
    render: &dyn Render,
    chain: &ChainPolicy,
    zone: crate::zone::Zone,
    set: &ChainSet<'_>,
    id: &str,
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
    let refs = view::references(&rows, &by_hash);
    let back = view::backlinks(&refs);
    let neighbours = |named: Option<&Vec<String>>| -> Vec<wire::Neighbour> {
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
    let content =
        view::content_of(row, &by_hash).map(|record| view::content(render, record, untrusted));
    let why = view::explain(row, at, &env).map(reading::Explained::of);
    let reading = render
        .page(&reading::todo(&entry, content.as_ref(), why.as_ref(), zone))
        .ok()
        .map(|html| html.as_str().to_owned());
    Ok(wire::TodoPayload {
        todo: id.to_owned(),
        at: fpl::iso(fpl::instant_of(at)),
        entry,
        reading,
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
    let mut briefs: Vec<crate::secretary::briefs::Brief> = crate::secretary::list(conversations)?
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
    let brief = crate::secretary::list(conversations)?
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
        Err(crate::capability::MailError::Absent { .. }) => {
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
        crate::capability::MailError::Absent { .. } => Refusal::not_found(format!(
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
) -> Answered<Vec<wire::Entry>> {
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
) -> Answered<wire::ObjectsPayload> {
    answered(
        store,
        &format!("chain/{}", chain.name.as_str()),
        books,
        since,
        now,
        Cut::Whole,
        &Literals,
    )
    .map(|(snapshot, piece)| wire::ObjectsPayload {
        untrusted: chain.untrusted_names(),
        root: Some(chain.root.as_str().to_owned()),
        ..snapshot.payload(piece.sent)
    })
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

/// `GET /api/stores/<store>/objects?since=<frontier>`: what a replica of `store` lacks,
/// cut as `cut` says: the objects the frontier `since` names does not reach, parents first,
/// every head, and how many objects are left after these. No replica writes to the store and no
/// policy reads it, so its untrusted set is empty, and it locks no words, so it carries no keys.
///
/// Every object comes after its parents, so a piece joined to what the caller holds holds the
/// ancestors of everything in it, and the pieces asked for one after another, each from the
/// frontier the last named, are together what one answer of the whole would send.
///
/// # Errors
/// The store's own, and the books'.
pub fn store_objects_since<E: crate::store::Store>(
    store: &dyn Prodrome<E>,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
    cut: Cut,
) -> Answered<wire::StorePayload> {
    let (snapshot, piece) = answered(
        store,
        &format!("store/{}", E::KIND.name()),
        books,
        since,
        now,
        cut,
        &Literals,
    )?;
    Ok(wire::StorePayload {
        objects: snapshot.payload(piece.sent),
        keys: BTreeMap::new(),
        rest: piece.rest,
        size: piece.size,
        weight: piece.weight,
        erased: Vec::new(),
    })
}

/// One object as a replica carries it, weighed by its literal's bytes.
fn stored<E: prodrome_core::schema::Schema>(
    name: &prodrome_core::event::Hash,
    envelope: &prodrome_core::event::Envelope<E>,
) -> (wire::StoredObject, usize) {
    let literal = prodrome_core::event::canonical_envelope(envelope);
    let bytes = literal.len();
    (
        wire::StoredObject {
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
    since: wire::SinceRead,
    frontier: wire::Frontier,
}

impl Snapshot {
    /// An objects answer of these objects, read under no policy: the caller's.
    fn payload(self, objects: Vec<wire::StoredObject>) -> wire::ObjectsPayload {
        wire::ObjectsPayload {
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
    sent: Vec<T>,
    rest: usize,
    weight: usize,
    size: usize,
}

/// What a pull sends of each object it answers: the object with its literal, or a skeleton's
/// node. An object past a piece's cut is only weighed, for the pull's whole size.
trait Sends<E: prodrome_core::schema::Schema> {
    /// One object as the answer carries it.
    type Item;
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
    type Item = wire::StoredObject;

    fn emit(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<E>,
    ) -> (wire::StoredObject, usize) {
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
    type Item = wire::SkeletonNode;

    fn emit(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<crate::secretary::line_event::LineEvent>,
    ) -> (wire::SkeletonNode, usize) {
        let node = skeleton_node(name, envelope);
        let weight = weight(&node);
        (node, weight)
    }

    fn weigh(
        &self,
        name: &prodrome_core::event::Hash,
        envelope: &prodrome_core::event::Envelope<crate::secretary::line_event::LineEvent>,
    ) -> usize {
        weight(&skeleton_node(name, envelope))
    }
}

/// An answer of `store`, read under its epoch's lock so the epoch and the objects are one
/// snapshot: every head, the prodromes it serves, and what `sends` makes of each object the
/// frontier `since` names does not reach, cut as `cut` says by the weight it gives each, in
/// linearisation order so a receiver can apply them as they arrive. The frontier the asker holds
/// once it has taken the answer is recorded, and named in it.
fn answered<E: prodrome_core::schema::Schema, S: Sends<E>>(
    store: &dyn Prodrome<E>,
    key: &str,
    books: Books<'_>,
    since: &prodrome_core::store::frontier::Since,
    now: Datetime,
    cut: Cut,
    sends: &S,
) -> Answered<(Snapshot, Piece<S::Item>)> {
    let (epoch, (heads, geneses, read, frontier, piece)) = books.epochs.served(key, now, || {
        let objects = store.objects()?;
        let dag: prodrome_core::dag::Dag<E> = objects.iter().cloned().collect();
        let heads = head_set(store)?;
        let geneses = names(&dag.geneses());
        let seen = crate::epoch::Seen {
            geneses: geneses.iter().cloned().collect(),
            heads: heads.iter().cloned().collect(),
            held: objects
                .iter()
                .map(|(name, _)| name.as_str().to_owned())
                .collect(),
        };
        let lacking = books.frontiers.answer(key, since, &dag)?;
        let read = since_read(lacking.reached());
        let mut piece = Piece {
            sent: Vec::new(),
            rest: 0,
            weight: 0,
            size: 0,
        };
        let mut full = false;
        for &(name, envelope) in lacking.lacking() {
            piece.rest += 1;
            if full {
                piece.size += sends.weigh(name, envelope);
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
        let piece = Piece {
            sent: piece.sent.into_iter().map(|(_, item)| item).collect(),
            rest: piece.rest,
            weight: piece.weight,
            size: piece.size,
        };
        Ok((seen, (heads, geneses, read, frontier, piece)))
    })?;
    Ok((
        Snapshot {
            heads,
            geneses,
            epoch,
            since: read,
            frontier,
        },
        piece,
    ))
}

/// Whether the store of record knew the frontier a pull named.
fn since_read(reached: &prodrome_core::store::frontier::Reached) -> wire::SinceRead {
    match reached {
        prodrome_core::store::frontier::Reached::Everything
        | prodrome_core::store::frontier::Reached::Known(_) => wire::SinceRead::Known,
        prodrome_core::store::frontier::Reached::Unknown => wire::SinceRead::Unknown,
    }
}

/// The frontier an answer recorded, as the wire names it.
fn frontier_of(recorded: &prodrome_core::store::frontier::Recorded) -> wire::Frontier {
    wire::Frontier {
        digest: recorded.token.as_str().to_owned(),
        count: recorded.count,
    }
}

/// `GET /api/stores/lines/objects?since=<heads>`: [`store_objects_since`] for the conversations'
/// lines, carrying the key of every slot the objects sent name, except a key an erasure of its
/// own conversation names, which is never sent: so a replica joining the answer can open what it
/// was sent and nothing erased. The erasures are read over the whole store, so a piece that
/// holds a line and not the later line erasing its words carries no key for them either.
///
/// # Errors
/// The stores' own.
pub fn lines_since(
    conversations: crate::secretary::conversations::Conversations<'_>,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
    cut: Cut,
) -> Answered<wire::StorePayload> {
    let payload = store_objects_since(conversations.lines, since, books, now, cut)?;
    let KeysSaid { keys, erased, .. } = words(
        conversations,
        payload
            .objects
            .objects
            .iter()
            .map(|object| object.hash.as_str()),
    )?;
    Ok(wire::StorePayload {
        keys,
        erased,
        ..payload
    })
}

/// `GET /api/stores/lines/skeleton?since=<heads>`: what [`lines_since`] answers, each object as
/// its [`wire::SkeletonNode`] instead of its literal, cut by the nodes' own weight: the same
/// heads, prodromes and epoch, read under the same lock, the same keys and the same erasures. A
/// replica that takes the pieces of the skeleton and the literal of each node by hash
/// ([`content`]) holds what the pieces of the objects' pull would have brought it.
///
/// # Errors
/// The stores' own.
pub fn lines_skeleton(
    conversations: crate::secretary::conversations::Conversations<'_>,
    since: &prodrome_core::store::frontier::Since,
    books: Books<'_>,
    now: Datetime,
    cut: Cut,
) -> Answered<wire::SkeletonPayload> {
    let (snapshot, piece) = answered(
        conversations.lines,
        "store/lines",
        books,
        since,
        now,
        cut,
        &Nodes,
    )?;
    let KeysSaid {
        keys,
        erased,
        replaced,
    } = words(
        conversations,
        piece.sent.iter().map(|node| node.hash.as_str()),
    )?;
    let nodes = held_sizes(conversations.lines, piece.sent)?;
    Ok(wire::SkeletonPayload {
        heads: snapshot.heads,
        geneses: snapshot.geneses,
        epoch: snapshot.epoch,
        nodes,
        since: snapshot.since,
        frontier: snapshot.frontier,
        keys,
        rest: piece.rest,
        size: piece.size,
        weight: piece.weight,
        erased,
        replaced,
    })
}

/// Each node sized by the bytes [`content`] sends of it, which are the store's own print and not
/// its reprint, so a tab that batches its asks by these sizes never asks for more than one answer
/// carries.
fn held_sizes(
    lines: &dyn Prodrome<crate::secretary::line_event::LineEvent>,
    nodes: Vec<wire::SkeletonNode>,
) -> Answered<Vec<wire::SkeletonNode>> {
    let names: Vec<prodrome_core::event::Hash> = nodes
        .iter()
        .map(|node| prodrome_core::event::Hash::new(node.hash.clone()))
        .collect::<Result<_, _>>()
        .map_err(|failed| crate::store::StoreError::unreadable("lines", failed.to_string()))?;
    let sizes = lines.sizes(&names)?;
    Ok(nodes
        .into_iter()
        .zip(sizes)
        .map(|(node, size)| wire::SkeletonNode {
            size: size.unwrap_or(node.size),
            ..node
        })
        .collect())
}

/// What a pull of the lines says of their words' keys.
struct KeysSaid {
    /// The key of every slot the lines sent name, by name, but a key an erasure names or a reseal
    /// replaced.
    keys: BTreeMap<String, String>,
    /// Every key an erasure anywhere in the store names, sorted.
    erased: Vec<String>,
    /// Every key a reseal that opens on the box supersedes, anywhere in the store, sorted
    /// (`transcript::superseded`).
    replaced: Vec<String>,
}

/// What the lines named `sent` and the store say of their keys.
fn words<'a>(
    conversations: crate::secretary::conversations::Conversations<'_>,
    sent: impl Iterator<Item = &'a str>,
) -> Answered<KeysSaid> {
    let objects = conversations.lines.objects()?;
    let erased = crate::secretary::transcript::erased(&objects);
    let replaced = crate::secretary::transcript::superseded(
        objects
            .iter()
            .filter_map(|(name, object)| Some((name, object.event()?))),
        &conversations.slots(),
    );
    let sent: std::collections::BTreeSet<&str> = sent.collect();
    let keys = objects
        .iter()
        .filter(|(name, _)| sent.contains(name.as_str()))
        .filter_map(|(_, object)| object.event())
        .flat_map(crate::secretary::line_event::LineEvent::keys)
        .filter(|name| !erased.contains_key(name) && !replaced.contains(name))
        .filter_map(|name| {
            let key = conversations.keys.get(name).ok()??;
            Some((
                name.as_str().to_owned(),
                crate::secretary::key_ring::spelled(&key),
            ))
        })
        .collect();
    Ok(KeysSaid {
        keys,
        erased: erased.keys().map(|name| name.as_str().to_owned()).collect(),
        replaced: replaced
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect(),
    })
}

/// One line's node: its name, what it rests on, its literal's size, and its header.
#[must_use]
pub fn skeleton_node(
    name: &prodrome_core::event::Hash,
    envelope: &prodrome_core::event::Envelope<crate::secretary::line_event::LineEvent>,
) -> wire::SkeletonNode {
    let header = match envelope.event() {
        Some(line) => wire::NodeHeader::Line {
            conversation: line.conversation().as_str().to_owned(),
            at: fpl::iso(fpl::instant_of(line.at())),
            kind: line.shape().name().to_owned(),
            keys: line.keys().map(|key| key.as_str().to_owned()).collect(),
            step: crate::secretary::step::Step::of(line.shape()),
        },
        None => wire::NodeHeader::Structure {
            kind: constructor(envelope).to_owned(),
        },
    };
    wire::SkeletonNode {
        hash: name.as_str().to_owned(),
        parents: prodrome_core::event::parents_of(envelope)
            .iter()
            .map(|parent| parent.as_str().to_owned())
            .collect(),
        size: prodrome_core::event::canonical_envelope(envelope).len(),
        header,
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
fn weight(node: &wire::SkeletonNode) -> usize {
    const AROUND: usize = 96;
    // A step's tag, and the count two of them carry.
    const STEP: usize = 32;
    let header = match &node.header {
        wire::NodeHeader::Structure { kind } => kind.len(),
        wire::NodeHeader::Line {
            conversation,
            at,
            kind,
            keys,
            step: _,
        } => {
            conversation.len()
                + at.len()
                + kind.len()
                + keys.iter().map(|key| key.len() + 3).sum::<usize>()
                + STEP
        }
    };
    AROUND + node.hash.len() + node.parents.iter().map(|p| p.len() + 3).sum::<usize>() + header
}

/// The most bytes of literals one content answer of more than one hash carries: a piece's
/// ([`Cut::PIECE`]). A single object is answered whatever its size, since a line written before
/// a stored answer was bounded can be larger.
pub const MOST_CONTENT_BYTES: usize = 1 << 20;

/// `GET /api/stores/<store>/content?hashes=` and a chain's `…/content?hashes=`: the literal of
/// each object `hashes` names, in that order, read from `store` alone. A hash the store does not
/// hold is refused, whether no store holds it or another store does, in the same words, and so
/// is a set whose literals together pass [`MOST_CONTENT_BYTES`]: either refuses the whole
/// question. An answer depends on nothing but the hashes, so it never changes.
///
/// # Errors
/// [`Refusal::not_found`] naming the hashes the store does not hold;
/// [`Refusal::bad_request`] for literals past the bound; the store's own.
pub fn content<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
    hashes: &query::Hashes,
) -> Answered<wire::ContentPayload> {
    let names: Vec<prodrome_core::event::Hash> = hashes.iter().cloned().collect();
    let literals = store.literals(&names)?;
    let unheld: Vec<&str> = names
        .iter()
        .zip(&literals)
        .filter(|(_, literal)| literal.is_none())
        .map(|(name, _)| name.as_str())
        .collect();
    if !unheld.is_empty() {
        return Err(Refusal::not_found(format!(
            "this store holds no object named {}",
            unheld.join(", ")
        )));
    }
    let sent: Vec<wire::StoredObject> = names
        .iter()
        .zip(literals)
        .filter_map(|(name, literal)| {
            literal.map(|literal| wire::StoredObject {
                hash: name.as_str().to_owned(),
                literal,
            })
        })
        .collect();
    let bytes: usize = sent.iter().map(|object| object.literal.len()).sum();
    if sent.len() > 1 && bytes > MOST_CONTENT_BYTES {
        return Err(Refusal::bad_request(format!(
            "these {} objects are {bytes} bytes, past the {MOST_CONTENT_BYTES} one answer carries: ask for fewer at a time, by the sizes the skeleton gives",
            sent.len()
        )));
    }
    Ok(wire::ContentPayload { objects: sent })
}

/// `GET /api/waiting`: how many cards the Inbox draws, as the page resolves its rows
/// (`web/src/inbox/inbox.ts`, `rowOf`), the one count the badge shows on every page, the Inbox's
/// included: one per open proposal, one per open decision on each of `chains`, and one per
/// proposal line no inbox record places. A tab reads this number without holding the
/// conversations: their lines are read by shape, never opened.
///
/// # Errors
/// A store that does not read, a chain the set does not hold, and lines the fold refuses: no
/// count is better than a wrong one.
pub fn waiting(
    inbox: &crate::secretary::inbox::Fold,
    lines: &dyn Prodrome<crate::secretary::line_event::LineEvent>,
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
                        && wire::EntryState::from(crate::believed::one_outcome(row.outcome()))
                            == wire::EntryState::Open
                })
            })
            .count();
    }
    let missing = |_: &crate::secretary::locked_text::KeyName| -> crate::secretary::words::Words<
        crate::secretary::locked_text::TextKey,
    > { crate::secretary::words::Gone::Missing.into() };
    let unplaced: usize = crate::secretary::transcript::transcripts(&lines.nodes()?, &missing)
        .map_err(|refused| Refusal::internal(refused.to_string()))?
        .into_values()
        .map(|read| read.into_stored().lines(inbox).unplaced().count())
        .sum();
    Ok(wire::WaitingPayload {
        count: proposals + decisions + unplaced,
    })
}

/// The most objects one post may carry; more is a refusal, never a silent truncation.
pub const MAX_POSTED: usize = 512;

/// `POST /api/objects`: the one write path, deciding under the write lock and then taking in
/// what the outcome names.
pub fn receive_objects(
    store: &dyn Prodrome<TodoEvent>,
    render: &dyn Render,
    chain: &ChainPolicy,
    posted: &Posted<'_>,
    frontiers: &prodrome_core::store::frontier::Frontiers,
    actor: &prodrome_core::event::Actor,
) -> Answered<(wire::ReceiptPayload, Vec<crate::secretary::acts::ActLine>)> {
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
    store.accept(&outcome.taken)?;
    // Render what landed before answering; `renders` keeps untrusted records uncompiled.
    let fresh: Vec<crate::escape::Escaped> = outcome
        .taken
        .iter()
        .filter_map(|(_, literal)| {
            prodrome_core::event::parse_envelope(&crate::record::TodoVocabulary::default(), literal)
                .ok()
        })
        .filter_map(|envelope| match envelope.event() {
            Some(TodoEvent::Authored(record)) => Some(record.clone()),
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
    // Adoption is the only moment a sealed lifecycle act's note is knowable server-side.
    let heard = crate::secretary::acts::heard_from_objects(
        &outcome
            .taken
            .iter()
            .filter_map(|(_, literal)| {
                prodrome_core::event::parse_envelope(
                    &crate::record::TodoVocabulary::default(),
                    literal,
                )
                .ok()
            })
            .filter_map(|envelope| envelope.event().cloned())
            .collect::<Vec<TodoEvent>>(),
        actor,
        &chain.name,
    );
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
/// More than [`MAX_POSTED`] objects, or the store not read or not written.
pub fn receive_changes<E: crate::store::Store>(
    store: &dyn Prodrome<E>,
    posted: &Posted<'_>,
    frontiers: &prodrome_core::store::frontier::Frontiers,
    actor: &prodrome_core::event::Actor,
) -> Answered<wire::ReceiptPayload>
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
) -> Answered<wire::ReceiptPayload> {
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
    Ok(wire::ReceiptPayload {
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
            .map(|refused| wire::RefusedObject {
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
) -> Answered<wire::AboutPayload> {
    let completes = released.map_or(&[][..], |r| r.completes.as_slice());
    let mut held = held_todos(plan, set, completes)?.unwrap_or_default();
    Ok(wire::AboutPayload {
        version: released.map_or_else(|| "dev".to_owned(), |r| r.version.to_string()),
        build: build.to_owned(),
        completes: completes
            .iter()
            .map(|todo| wire::Shipped {
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
) -> Answered<Option<std::collections::BTreeMap<String, wire::HeldTodo>>> {
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
                    wire::HeldTodo {
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
    admitted: &crate::gate::Admitted,
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
    caps: &crate::capability::Server<'_>,
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
pub const APP_RATIONALE: &str = "Set by hand in the app.";
/// An action line, not a document.
pub const MAX_BODY_CHARS: usize = 2000;
/// A sentence.
pub const MAX_RATIONALE_CHARS: usize = 500;
/// A screen and a half, generously.
pub const MAX_DETAIL_CHARS: usize = 8000;
/// A sentence.
pub const MAX_WAITING_ON_CHARS: usize = 500;
/// A checklist that long is a mistake, not a todo.
pub const MAX_SUBTODOS: usize = 200;

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

/// `GET /api/usage`: what this box has spent of its model subscription's allowance, over rolling windows on
/// the wall clock.
///
/// # Errors
/// The transcripts being unreadable.
pub fn usage(
    usage: &dyn crate::capability::Usage,
    at: chrono::DateTime<chrono::Utc>,
) -> Answered<crate::usage::Report> {
    let requests = usage
        .requests()
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let refusals = usage
        .refusals()
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let allowances = usage.allowance();
    let scope = usage
        .scope()
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    Ok(crate::usage::report(
        at,
        &requests,
        &refusals,
        &allowances,
        usage.horizon(),
        scope,
    ))
}

/// `GET /api/overview`: what is connected, by group, read from `declared` and what the box's
/// stores say of it now. The secret store is asked once for every declared name.
#[must_use]
pub fn overview(
    declared: &crate::settings::overview::Declared<'_>,
    secrets: &dyn crate::capability::Secrets,
    observed: &crate::settings::overview::Registers<'_>,
    asking: &[crate::settings::overview::Asking],
) -> wire::Overview {
    let settables = declared.settables;
    let snapshot = crate::capability::Snapshot::read(secrets, settables.iter());
    let states: BTreeMap<String, SecretState> = settables
        .iter()
        .map(|settable| (settable.name().to_owned(), snapshot.state(&settable)))
        .collect();
    let grants = declared
        .accounts
        .iter()
        .map(|account| (account.email.to_string(), secrets.mail_grants(account)))
        .collect();
    let observed = crate::settings::overview::Observed {
        secrets: &states,
        grants: &grants,
        registers: *observed,
    };
    crate::settings::overview::overview(declared, &observed).wire(
        &settables.names(),
        asking,
        crate::secretary::noticed::CONVERSATION.as_str().to_owned(),
    )
}

/// `GET /api/calendar/<date>`: a week of the owner's calendars from that day, as Typst's HTML
/// and as plain text.
pub fn calendar(
    calendar: &dyn crate::capability::Calendar,
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
        rich: render
            .page(&crate::reading::calendar(&days))
            .ok()
            .map(|html| html.as_str().to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value the fake's store reads as locked: it holds the name and cannot say what it is.
    const LOCKED: &str = "<the store is locked>";

    /// A fake `Secrets` holding real-shaped values, so a leak would show.
    struct FakeSecrets {
        values: BTreeMap<String, String>,
        grants: BTreeMap<String, crate::capability::MailGrants>,
    }

    impl crate::capability::Secrets for FakeSecrets {
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
        ) -> crate::capability::MailGrants {
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
        &'a dyn crate::capability::Secrets,
        std::sync::atomic::AtomicUsize,
    );

    impl crate::capability::Secrets for Counting<'_> {
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
        ) -> crate::capability::MailGrants {
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
        build: wire::AboutPayload,
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
                build: wire::AboutPayload {
                    version: "dev".to_owned(),
                    build: "dev".to_owned(),
                    completes: Vec::new(),
                },
            }
        }

        fn overview(&self, secrets: &dyn crate::capability::Secrets) -> wire::Overview {
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
                crate::plugin::PluginName::parse("ics").expect("a name"),
                crate::settings::overview::Store::Calendar,
            )]);
            overview(
                &declared,
                secrets,
                &crate::settings::overview::Registers {
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

    fn group(page: &wire::Overview, which: crate::connect::Group) -> &[wire::OverviewLine] {
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
