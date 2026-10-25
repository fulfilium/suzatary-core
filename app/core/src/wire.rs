//! The web's payloads, with serde; their TypeScript is generated into
//! `web/src/wire/generated/wire.ts`, and the browser's decoder of each answer
//! into `payload.ts` beside it, folded from that text ([`shape`]). A field name
//! here is the name on the wire, so renaming one is a contract change. Domain
//! types that are already the wire
//! derive `TS` where they live; this file holds the pages and views made for it.
//! Request bodies are declared as the server accepts them, defaults included.

pub mod shape;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::{Dependency, TS, TypeVisitor};

use crate::reference::Reference;

/// One stored object: an event about a todo, or an envelope that carries none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Link {
    /// The object's hash, which `literal` rehashes to.
    pub hash: String,
    /// Every object this one rests on; none at a genesis.
    pub parents: Vec<String>,
    /// The canonical literal, the bytes the store holds.
    pub literal: String,
    /// What it holds.
    #[serde(flatten)]
    #[ts(flatten)]
    pub holds: Holds,
}

/// What a stored object holds: an envelope with no event (a merge, a genesis, a signature), or an
/// event, with who wrote it and when. An envelope claims no todo, writer or instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "holds", rename_all = "lowercase")]
pub enum Holds {
    /// No event.
    Envelope {
        /// The envelope's constructor.
        kind: String,
    },
    /// An event about one todo.
    Event {
        /// The event's constructor.
        kind: String,
        /// The todo it is about.
        todo: String,
        /// Who appended it.
        actor: String,
        /// When, ISO 8601.
        at: String,
        /// The event's own note, as written.
        note: String,
        /// What it reverses, read once from its note.
        reverses: Reverses,
    },
}

/// What a stored event reverses: nothing, or the object an undo or a redo names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Reverses {
    /// An ordinary event.
    Nothing,
    /// The inverse of `of`.
    Undo {
        /// The object it inverts, by hash.
        of: String,
    },
    /// The object the undo `of` undid, again.
    Redo {
        /// The undo it reverses, by hash.
        of: String,
    },
}

impl From<Option<crate::authoring::Reversal>> for Reverses {
    fn from(reversal: Option<crate::authoring::Reversal>) -> Self {
        use crate::authoring::Reversal;
        match reversal {
            None => Self::Nothing,
            Some(Reversal::Undo { of }) => Self::Undo {
                of: of.as_str().to_owned(),
            },
            Some(Reversal::Redo { of }) => Self::Redo {
                of: of.as_str().to_owned(),
            },
        }
    }
}

/// The message a todo was authored from; `""` where its era recorded nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct AuthoredSource {
    /// The From header.
    pub sender: String,
    /// The Subject header.
    pub subject: String,
    /// The message's date, ISO 8601 date only.
    pub date: String,
    /// A prefix of the message's canonical hash; `/api/mail/message/<hash>` resolves it.
    pub hash: String,
    /// The notmuch thread id.
    pub thread_id: String,
    /// The RFC 5322 Message-ID.
    pub message_id: String,
}

/// One line of a checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubTodo {
    /// Its action text, typst markup.
    pub body: String,
    /// Whether it is done.
    pub done: bool,
}

/// Typst's HTML for the prose fields. `null` when absent, not compiled,
/// refused, or not a trusted actor's (rendered means confirmed); the page
/// then shows the source, which always travels beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Rich {
    /// The action line.
    pub body: Option<String>,
    /// The detail.
    pub detail: Option<String>,
    /// The rationale, rejoined.
    pub rationale: Option<String>,
    /// The waiting-on, from string source.
    pub waiting_on: Option<String>,
    /// The key each present field's HTML is kept under, rendered or not, `null` for an absent or untrusted one.
    pub keys: RichKeys,
}

/// Each field's key in the tabulation of rendered HTML ([`crate::rendering::key`] of the markup it renders): the
/// core names it for the box and the tab alike, so neither derives the other's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct RichKeys {
    /// The action line's.
    pub body: Option<String>,
    /// The detail's.
    pub detail: Option<String>,
    /// The rationale's, rejoined and escaped.
    pub rationale: Option<String>,
    /// The waiting-on's, escaped.
    pub waiting_on: Option<String>,
}

/// What a content record is drawn as, read once from its constructor's name: a decision is a
/// choice awaiting the owner's tap and never a todo on a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Block {
    /// `#decision(..)`.
    Decision,
    /// Any other constructor: `todo`, or one whose name files it (`ekaTodo`).
    Todo {
        /// The constructor's name.
        name: String,
    },
}

impl Block {
    /// The block a record's constructor `name` draws.
    #[must_use]
    pub fn of(name: &str) -> Self {
        match name {
            "decision" => Self::Decision,
            other => Self::Todo {
                name: other.to_owned(),
            },
        }
    }

    /// The constructor's name, as a chip prints it.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Decision => "decision",
            Self::Todo { name } => name,
        }
    }
}

/// A todo's content as the confirmed fold sees it. `""` is absent.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Authored {
    /// Typst's HTML beside the source.
    pub rich: Rich,
    /// What the record is drawn as: a decision, or a todo by its block constructor.
    pub block: Block,
    /// ISO 8601 date only.
    pub created: String,
    /// The action line, typst markup.
    pub body: String,
    /// The action line as words: [`crate::escape::markup_text`] of its markup, never rendered, so a row shows an
    /// unconfirmed line as text and not as the markup it was written in.
    pub text: String,
    /// The detail, typst markup.
    pub detail: String,
    /// The category.
    pub category: String,
    /// What it is blocked on, typst string source.
    pub waiting_on: String,
    /// The `//` comment above the priority, line by line.
    pub rationale: Vec<String>,
    /// The authored FPL spec, printed by [`crate::term_json::to_json`].
    #[ts(type = "SpecNode | null")]
    pub spec: Option<serde_json::Value>,
    /// Who wrote it.
    pub actor: String,
    /// ISO 8601.
    pub at: String,
    /// The message it came from.
    pub source: Option<AuthoredSource>,
    /// The checklist.
    pub subtodos: Vec<SubTodo>,
}

/// A todo's confirmed lifecycle. The `From` matches `Outcome` with no
/// wildcard, so a new binding fails the build instead of reading as open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum EntryState {
    /// No binding.
    Open,
    /// `Outcome::Completed`.
    Completed,
    /// `Outcome::Cancelled`.
    Cancelled,
}

impl From<Option<prodrome_core::fpl::Outcome>> for EntryState {
    fn from(outcome: Option<prodrome_core::fpl::Outcome>) -> Self {
        match outcome {
            None => EntryState::Open,
            Some(prodrome_core::fpl::Outcome::Completed(_)) => EntryState::Completed,
            Some(prodrome_core::fpl::Outcome::Cancelled(_)) => EntryState::Cancelled,
        }
    }
}

/// What an untrusted actor's event would bind where the confirmed fold
/// disagrees. Agreement is spelled `""`, which the stored pins print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum ClaimedState {
    /// Nothing is claimed.
    #[serde(rename = "")]
    Agrees,
    /// A claimed completion.
    #[serde(rename = "completed")]
    Completed,
    /// A claimed cancellation.
    #[serde(rename = "cancelled")]
    Cancelled,
}

impl From<Option<prodrome_core::fpl::Outcome>> for ClaimedState {
    fn from(claim: Option<prodrome_core::fpl::Outcome>) -> Self {
        match claim {
            None => ClaimedState::Agrees,
            Some(prodrome_core::fpl::Outcome::Completed(_)) => ClaimedState::Completed,
            Some(prodrome_core::fpl::Outcome::Cancelled(_)) => ClaimedState::Cancelled,
        }
    }
}

/// A fulfillment as prodrome-wasm spells it: a number in [0, 1], or
/// `"absent"` for `∅`. A todo nothing prices is not a todo worth nothing.
/// Where the term may also fail to link it is `Option<Fulfillment>`, `null`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Fulfillment {
    /// Low is urgent.
    Number(f64),
    /// `∅`.
    Absent(Absent),
}

/// `∅`'s one spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Absent {
    /// `"absent"`.
    #[serde(rename = "absent")]
    Absent,
}

impl Fulfillment {
    /// The core's reading: a number, `∅`, or `None` where it does not link.
    #[must_use]
    pub fn of<E>(value: Result<Option<f64>, E>) -> Option<Fulfillment> {
        value.ok().map(Fulfillment::of_value)
    }

    /// A value that links.
    #[must_use]
    pub fn of_value(value: Option<f64>) -> Fulfillment {
        value.map_or(Fulfillment::Absent(Absent::Absent), Fulfillment::Number)
    }

    /// The number, or `None` for `∅`.
    #[must_use]
    pub fn number(self) -> Option<f64> {
        match self {
            Fulfillment::Number(value) => Some(value),
            Fulfillment::Absent(Absent::Absent) => None,
        }
    }
}

/// One todo as the folds see it at a moment.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Entry {
    /// The todo's id.
    pub todo: String,
    /// The confirmed binding a list files it under: open while any candidate is.
    pub state: EntryState,
    /// Every candidate binding, one unless the state is in conflict.
    pub states: Vec<EntryState>,
    /// When it was resolved, `""` while open.
    pub at: String,
    /// Shown, never applied: the containment asymmetry made visible.
    pub claimed: ClaimedState,
    /// The fulfillment at the fold's instant; `null` where it does not link.
    pub value: Option<Fulfillment>,
    /// A pass restarts its price (`Tended`, "Did it") rather than completing it.
    pub recurring: bool,
    /// Provisional content, or a claim the trusted fold refuses.
    pub unconfirmed: bool,
    /// Registers with more than one live write, each write by hash; the next
    /// write settles it. `Record`, not ts-rs's optional-valued map: a
    /// `BTreeMap` never serialises an absent value.
    #[ts(type = "Record<string, string[]>")]
    pub conflicts: BTreeMap<String, Vec<String>>,
    /// The content, or `null` for a todo mentioned and never authored; under a content conflict
    /// the first of `candidates`.
    pub authored: Option<Authored>,
    /// Every candidate record, by object name, where the content is in conflict; else empty.
    pub candidates: Vec<Authored>,
    /// Its objects in causal order, by hash.
    pub stream: Vec<String>,
}

/// A todo as a neighbour row reads it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Neighbour {
    /// The todo's id.
    pub todo: String,
    /// Its state.
    pub state: EntryState,
    /// Its fulfillment.
    pub value: Option<Fulfillment>,
    /// Whether it is provisional.
    pub unconfirmed: bool,
}

/// One field whose canonical print changed, checkable against the stored object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct FieldChange {
    /// The field.
    pub field: String,
    /// Its print before, `""` for the first record.
    pub before: String,
    /// Its print after.
    pub after: String,
}

/// `GET /api/chain`: every object in the linearisation's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ChainPayload {
    /// Every head, sorted.
    pub heads: Vec<String>,
    /// This deployment's trust policy, so an offline fold folds the same chain.
    pub untrusted: Vec<String>,
    /// The root a device key counts under: whose word each object is, read in the browser.
    pub root: String,
    /// The fsck's findings.
    pub verify: Vec<String>,
    /// The objects.
    pub links: Vec<Link>,
}

/// `GET /api/fold`: every todo the chain has mentioned, at `t`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct FoldPayload {
    /// The instant, ISO 8601.
    pub at: String,
    /// The heads of the objects this fold read, sorted: a tab keeps whichever of two folds read
    /// more.
    pub heads: Vec<String>,
    /// One row per todo, by id.
    pub entries: Vec<Entry>,
    /// Every chain the set holds as a failure: what refers into one reads `∅` until it reads.
    pub failed: Vec<FailedChain>,
    /// When a row's whole percent, or what the fold believes, next moves after `at`; `null` for
    /// never. A page is drawn again then, and not on a ticking clock.
    pub next: Option<NextChange>,
}

/// When a reading next moves: an instant in the reading's own zone, as `at` is, and whether
/// something moves there (`exact`) or it is a bound that may come before anything does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct NextChange {
    /// The instant, ISO 8601.
    pub at: String,
    /// Whether it is the change itself rather than a bound before it.
    pub exact: bool,
}

impl NextChange {
    /// The core's answer on the wire, `None` for never.
    #[must_use]
    pub fn of(next: prodrome_core::observe::NextChange) -> Option<NextChange> {
        next.at.map(|at| NextChange {
            at: prodrome_core::fpl::iso(at),
            exact: next.exact,
        })
    }
}

/// A chain the set holds as a failure, and why, as one line a page shows.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct FailedChain {
    /// The chain's name.
    pub chain: String,
    /// Why it is no member, in a sentence naming it.
    pub why: String,
}

/// Both directions of a todo's reference graph, derived from content.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TodoLinks {
    /// Todos it refers to.
    pub out: Vec<Neighbour>,
    /// Todos that refer to it.
    #[serde(rename = "in")]
    pub inward: Vec<Neighbour>,
}

/// `GET /api/todo/{id}`: one todo in full.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TodoPayload {
    /// The id.
    pub todo: String,
    /// The instant, ISO 8601.
    pub at: String,
    /// The entry.
    pub entry: Entry,
    /// The reading page as typst's HTML; `null` when typst did not render it.
    pub reading: Option<String>,
    /// The event stream in causal order.
    pub stream: Vec<Link>,
    /// What each `Authored` changed, by that event's hash.
    #[ts(type = "Record<string, FieldChange[]>")]
    pub diffs: BTreeMap<String, Vec<FieldChange>>,
    /// The reference graph around it.
    pub links: TodoLinks,
}

/// How `GET /api/tree` groups; ancestry nests under each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TreeBy {
    /// By the root's category.
    Category,
    /// Problem, Watch and Fine, by the root's value.
    Urgency,
    /// No groups.
    Flat,
}

/// What a group groups by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum GroupBy {
    /// The whole list.
    All,
    /// A category.
    Category,
    /// An urgency section.
    Urgency,
}

/// A node's value, or why its term does not link.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Worth {
    /// The fulfillment.
    Value(f64),
    /// `∅`.
    Absent,
    /// A `Ref` to a todo the chain has never seen.
    Unknown(String),
    /// A reference loop, its first todo repeated at the end.
    Cycle(Vec<String>),
}

/// One node of the list's tree; `term` prints what was evaluated, children ascend.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TreeNode {
    /// The `Conj` of its children.
    Group {
        /// What it groups by.
        by: GroupBy,
        /// The category or section, `""` for the root.
        label: String,
        /// Its value.
        worth: Worth,
        /// Its term, printed.
        term: String,
        /// What it holds.
        children: Vec<TreeNode>,
    },
    /// `Ref(todo)`, its subtodos as children.
    Todo {
        /// The id.
        todo: String,
        /// The action line, plain text.
        title: String,
        /// Its category, or its kind's.
        category: String,
        /// Its value.
        worth: Worth,
        /// Its term, printed.
        term: String,
        /// Its subtodos.
        children: Vec<TreeNode>,
    },
}

/// `GET /api/<chain>/tree`: the open todos as one tree of terms.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TreePayload {
    /// The instant, ISO 8601.
    pub at: String,
    /// How it is grouped.
    pub by: TreeBy,
    /// The whole list.
    pub root: TreeNode,
}

/// `GET /api/tip`: the box's heads alone, what a phone polls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct TipPayload {
    /// Every head, sorted: a set, never one of them picked.
    pub heads: Vec<String>,
}

/// One frame of `GET /events`: the heads, each time they move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct TipFrame {
    /// Every head, sorted.
    pub heads: Vec<String>,
}

/// A store a tab holds a replica of, as its path segment spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
pub enum StoreName {
    /// The proposals.
    #[serde(rename = "inbox")]
    Inbox,
    /// The conversations' lines.
    #[serde(rename = "lines")]
    Lines,
    /// The topics on rounds and notes.
    #[serde(rename = "topics")]
    Topics,
    /// The topics' aliases.
    #[serde(rename = "topic-aliases")]
    TopicAliases,
}

impl StoreName {
    /// Every store, in the order a stream first names them.
    pub const ALL: [StoreName; 4] = [
        StoreName::Inbox,
        StoreName::Lines,
        StoreName::Topics,
        StoreName::TopicAliases,
    ];

    /// Its row, whose name is its path segment.
    #[must_use]
    pub const fn kind(self) -> crate::store::Kind {
        use crate::store::Kind;
        match self {
            StoreName::Inbox => Kind::Inbox,
            StoreName::Lines => Kind::Lines,
            StoreName::Topics => Kind::Topics,
            StoreName::TopicAliases => Kind::TopicAliases,
        }
    }
}

/// One frame of `GET /api/stores/events`: one store's heads, each time they move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct StoreFrame {
    /// Which store.
    pub store: StoreName,
    /// Every head, sorted.
    pub heads: Vec<String>,
}

/// One saved briefing file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BriefingRow {
    /// The day, ISO 8601.
    pub date: String,
    /// Its first heading, or the date.
    pub title: String,
    /// How many words.
    pub words: u32,
}

/// One morning brief the secretary wrote, and where its answer is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BriefRow {
    /// The day, ISO 8601.
    pub date: String,
    /// The conversation.
    pub conversation: String,
    /// The answering turn.
    pub turn: u32,
    /// Its first line.
    pub title: String,
    /// How many words.
    pub words: u32,
    /// The answer's text.
    pub text: String,
}

/// `GET /api/briefings`: newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BriefingsPayload {
    /// The saved files, capped.
    pub briefings: Vec<BriefingRow>,
    /// The secretary's briefs, capped.
    pub briefs: Vec<BriefRow>,
}

/// `GET /api/briefings/{date}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BriefingPayload {
    /// The day, ISO 8601.
    pub date: String,
    /// What was written.
    pub day: BriefingDay,
}

/// A day's briefing: the secretary's brief when it wrote one, else the saved file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BriefingDay {
    /// The brief, block by block as the conversation draws it.
    Brief {
        /// The conversation.
        conversation: String,
        /// The answering turn.
        turn: u32,
        /// The answer's text.
        text: String,
        /// Its blocks.
        blocks: Vec<BlockRow>,
    },
    /// A file under `briefings/`, a trusted author's, so it renders unconditionally.
    Saved {
        /// Its typst markup.
        source: String,
        /// Typst's HTML, or `null`.
        rich: Option<String>,
    },
}

/// One message page: the message beside the todos it produced now.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "MailMessage")]
pub struct MailPage {
    /// The message.
    #[serde(flatten)]
    #[ts(flatten)]
    pub message: crate::mail::MailMessage,
    /// The todos it produced.
    pub todos: Vec<Entry>,
}

/// `GET /api/mail/inbox`: recent inbox mail by date; the roster rides along
/// so the filter chips are every account, not the page's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct InboxPayload {
    /// Every account.
    pub accounts: Vec<String>,
    /// The rows.
    pub messages: Vec<crate::mail::MailRow>,
}

/// `GET /api/mail/watches`, and what a cancel answers: every watch that waits, the soonest to
/// expire first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct WatchesPayload {
    /// The watches.
    pub watches: Vec<MailWatch>,
}

/// One watch on mail the owner expects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MailWatch {
    /// Its name, which a cancel takes.
    pub id: String,
    /// Which part of a message it matches.
    pub on: WatchOn,
    /// The text it looks for, lower-cased.
    pub text: String,
    /// When it was set, ISO.
    pub set: String,
    /// When it expires, ISO.
    pub until: String,
}

/// Which part of a message a watch matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum WatchOn {
    /// The sender's address or name.
    Sender,
    /// The subject.
    Subject,
}

/// `GET /api/mail/search`: a notmuch query, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MailSearchPayload {
    /// The query, echoed; it lives in the page's hash.
    pub q: String,
    /// Every account.
    pub accounts: Vec<String>,
    /// The rows.
    pub messages: Vec<crate::mail::MailRow>,
}

/// `GET /api/mail/thread/{id}`: oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ThreadPayload {
    /// The thread id.
    pub thread: String,
    /// Its messages.
    pub messages: Vec<MailPage>,
}

/// What `POST /api/author` may be asked for, by its one key, so a body naming two or none is
/// refused at the parse. A person's line travels inside the two arms that write it.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Authoring {
    /// A new todo.
    Add(Box<NewTodo>),
    /// A change to a todo the chain already mentions.
    Update(Box<UpdateTodo>),
    /// A provisional record, re-appended by a trusted actor.
    Confirm(ConfirmTodo),
    /// The inverse of this todo's last event.
    Undo(String),
    /// The event an undo on this todo just undid, appended again.
    Redo(String),
}

/// `POST /api/author` `{"add": …}`: a new todo's fields, with the id its author names, if any,
/// and the todo it is a subtodo of. No actor (the gate names it) and no instant (the box stamps
/// it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NewTodo {
    /// What it says and what it is worth; an empty rationale gets the app's sentence.
    #[serde(flatten)]
    #[ts(flatten)]
    pub fields: crate::todo::TodoFields,
    /// An explicit id, refused if the chain ever mentioned it; otherwise one from the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    /// The todo this is a subtodo of; its spec gains `Ref(this)` in the same act.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<String>,
}

/// `POST /api/author` `{"update": …}`: a change to a todo the chain already mentions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateTodo {
    /// Which todo.
    pub todo: String,
    /// What changes; what is left out is kept.
    #[serde(flatten)]
    #[ts(flatten)]
    pub patch: crate::todo::TodoPatch,
    /// The person's line, which the event carries; `""` for none.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub note: String,
}

/// `POST /api/author` `{"confirm": …}`: a provisional record a trusted actor takes as its own.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConfirmTodo {
    /// Which todo.
    pub todo: String,
    /// The person's line, which the event carries; `""` for none.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub note: String,
}

/// `POST /api/act/device`: a push token is a fact about a phone, not the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct DevicePayload {
    /// Always true; a failure is a refusal.
    pub ok: bool,
    /// How many tokens this actor has on file.
    pub devices: u32,
}

/// One stored object as a replica carries it: the name and the bytes, not
/// the server's summary of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct StoredObject {
    /// sha256 of the literal, rehashed by the receiver.
    pub hash: String,
    /// The canonical print.
    pub literal: String,
}

/// The frontier a replica holds once it has taken an answer: the digest the box recorded its
/// heads under, which the replica names back as `since`, and how many objects lie under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Frontier {
    /// 64 hex digits.
    pub digest: String,
    /// The objects at or under the frontier's heads.
    pub count: usize,
}

/// How the box read a request's `since`: the frontier it names, or one it does not hold, which
/// it answered as a replica holding nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum SinceRead {
    /// Nothing, or a frontier the box recorded.
    Known,
    /// A frontier the box never recorded or let go.
    Unknown,
}

/// `GET /api/objects?since=<frontier>`: what the caller lacks, with the box's
/// heads and the trust policy it folds them under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ObjectsPayload {
    /// Every head, sorted.
    pub heads: Vec<String>,
    /// This deployment's untrusted actors.
    pub untrusted: Vec<String>,
    /// The root key whose signatures make a device key count, on a chain's objects: a tab that
    /// writes signs every object it seals, and reads who wrote what under it. `null` for a
    /// store the box alone writes.
    pub root: Option<String>,
    /// The genesis of every prodrome the store holds, sorted: a replica drops what it holds of any
    /// other, which is a store the box migrated away from.
    pub geneses: Vec<String>,
    /// The box's own clock over what the store serves (`suzatary_core::epoch`): bumped whenever
    /// `geneses` changes and by nothing else, so a replica takes the word of the latest epoch,
    /// a drop or a rollback included, whatever its objects reach.
    #[ts(type = "number")]
    pub epoch: u64,
    /// Parents first.
    pub objects: Vec<StoredObject>,
    /// How the box read `since`.
    pub since: SinceRead,
    /// What the replica holds once it has taken these, to name as the next `since`.
    pub frontier: Frontier,
}

/// `GET /api/stores/<store>/objects?since=<heads>`: what a replica of one of the secretary's
/// stores lacks, and the keys that open its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct StorePayload {
    /// The objects, as every store's pull answers them.
    #[serde(flatten)]
    #[ts(flatten)]
    pub objects: ObjectsPayload,
    /// The key of every slot of words the objects sent name, by the key's name, its bytes in
    /// hex: never a key an erasure of the same conversation names, so a replica that joins this
    /// answer can open what it was sent and no word erased. Empty for a store that locks no
    /// words.
    #[ts(type = "Record<string, string>")]
    pub keys: BTreeMap<String, String>,
    /// How many objects the replica still lacks after these: none when the answer holds all it
    /// lacked. The answer is a piece, and the replica asks again from the heads it holds once it
    /// has kept it.
    pub rest: usize,
    /// The bytes of the literals the replica lacked when it asked, these and the rest: what the
    /// first piece of a pull tells a tab its whole pull will bring.
    pub size: usize,
    /// The bytes of the literals this piece sends, so `size` less this is what is left.
    pub weight: usize,
    /// The name of every key an erasure anywhere in the store names, sorted: a replica forgets
    /// the blocks those keys' words were rendered to and drops the keys as soon as it takes any
    /// piece, before the piece holding the erasure arrives. Empty for a store that locks no words.
    pub erased: Vec<String>,
}

/// `GET /api/stores/<store>/content?hashes=<hashes>` and a chain's `…/content?hashes=`: the
/// literal of each object asked for, in the order asked. An object's literal is what its name is
/// the hash of, so the same question always gets the same answer, and the box sends it to be
/// kept for good (`Cache-Control: immutable`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ContentPayload {
    /// One per hash asked, in the order asked.
    pub objects: Vec<StoredObject>,
}

/// One object of a store's skeleton: its name, what it rests on, how large its literal is, and
/// what a fold reads of it without its literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SkeletonNode {
    /// The object's name, the sha256 of its literal.
    pub hash: String,
    /// Every object it rests on, its prodrome's genesis included, sorted.
    pub parents: Vec<String>,
    /// The bytes of its literal, so a tab can ask for content in answers of a bounded size.
    pub size: usize,
    /// What a fold reads of it.
    pub header: NodeHeader,
}

/// What a fold of the lines reads of one object without its literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "node", rename_all = "lowercase")]
pub enum NodeHeader {
    /// An object that carries no line: a genesis, a signature, a key.
    Structure {
        /// Its constructor, `Genesis` for instance.
        kind: String,
    },
    /// One line of one conversation.
    Line {
        /// The conversation it is a line of.
        conversation: String,
        /// When it was written, by its writer's clock.
        at: String,
        /// Its shape's constructor, `Said` for instance.
        kind: String,
        /// The name of the key of each slot of its words, in slot order.
        keys: Vec<String>,
    },
}

/// `GET /api/stores/lines/skeleton?since=<heads>`: the lines a replica holding those heads lacks,
/// each without its literal, parents first, cut like a piece of the objects' pull, with the same
/// heads, prodromes, epoch, keys and erasures that pull answers. Joined to the literals fetched by
/// hash, the pieces of a skeleton are what the objects' pull sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SkeletonPayload {
    /// Every head, sorted.
    pub heads: Vec<String>,
    /// The genesis of every prodrome the store holds, sorted.
    pub geneses: Vec<String>,
    /// The box's clock over what the store serves, the one the objects' pull answers with.
    #[ts(type = "number")]
    pub epoch: u64,
    /// Parents first.
    pub nodes: Vec<SkeletonNode>,
    /// How the box read `since`.
    pub since: SinceRead,
    /// What the replica holds once it has taken these, to name as the next `since`.
    pub frontier: Frontier,
    /// The key of every slot of words the nodes sent name, as the objects' pull carries them.
    #[ts(type = "Record<string, string>")]
    pub keys: BTreeMap<String, String>,
    /// How many objects the replica still lacks after these.
    pub rest: usize,
    /// The bytes of the nodes the replica lacked when it asked, these and the rest, by the weight
    /// a piece is cut by: what the first piece tells a tab the whole skeleton will bring.
    pub size: usize,
    /// The bytes of the nodes this piece sends, by the same weight, so `size` less this is what
    /// is left.
    pub weight: usize,
    /// The name of every key an erasure anywhere in the store names, sorted.
    pub erased: Vec<String>,
}

/// `POST /api/objects`: literals only; a name is what the bytes are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PostedObjects {
    /// Canonical prints, in any order.
    pub objects: Vec<String>,
    /// The frontier the poster held before these, as a pull names it: absent or empty for none.
    #[serde(default)]
    pub since: String,
}

/// `POST /api/secretary/conversations` and `…/title`: a topic's title; `""` leaves it to the
/// title job.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Titling {
    /// The title.
    pub title: String,
}

/// `POST …/say` and `…/edit`: a person's turn, the chips it names and the files it carries.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Saying {
    /// The words.
    pub say: String,
    /// Each chip, in its one text form (`todo:life/x`).
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub references: Vec<crate::reference::Reference>,
    /// Each file, by its id in the store.
    #[serde(default)]
    #[ts(as = "Option<Vec<PickedFile>>", optional)]
    pub attachments: Vec<PickedFile>,
    /// How many turns the asker saw; absent, a question is always a new line.
    #[serde(default)]
    #[ts(optional)]
    pub after: Option<usize>,
}

/// One file a turn names.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PickedFile {
    /// Its id in the store, 64 hex.
    #[ts(as = "String")]
    pub id: crate::attachment::AttachmentId,
    /// The name it was picked with, `""` for a photo.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub name: String,
}

/// `POST /api/secretary/attachments`: a photo or PDF, base64 in JSON so it passes the same
/// content-type rule as every write.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Uploading {
    /// The bytes, base64.
    pub file: String,
    /// The name it was picked with, `""` for a photo.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub name: String,
    /// The owner said to keep a file the box held back in his Drive.
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub private: bool,
}

/// `POST …/proposals/<n>/tap`: the owner's answer, which carries an index in its path and never
/// a body that could change what is performed.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Tapping {
    /// A line for the conversation, `""` for none.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub note: String,
    /// A secret's value, for a secret card only.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub value: Option<crate::capability::Secret>,
    /// The card's wider yes.
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub always: bool,
    /// The tapping device's signature of a registration, for a device card only.
    #[serde(default)]
    #[ts(optional)]
    pub endorsement: Option<Endorsing>,
}

/// A trusted device's signature of another's registration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Endorsing {
    /// The signing device's public key, 64 hex.
    pub key: String,
    /// The signature, 128 hex.
    pub signature: String,
}

/// `POST …/dismiss` and `…/drop`: why the owner said no.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Reasoning {
    /// The reason, which the secretary reads.
    pub reason: String,
}

/// `POST /api/act/key`: a device asking to be trusted.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KeyAsked {
    /// Its public key, 64 hex.
    pub key: String,
}

/// `POST /api/act/pair`: a device typing the code `suzatary device pair` printed at the box.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Pairing {
    /// The code, as typed.
    pub code: String,
}

/// `POST /api/held/reveal`: the owner's tap on a held value's placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Revealing {
    /// The handle, as the placeholder prints it (`held:…`).
    pub handle: String,
}

/// What `POST /api/held/reveal` answers. The box keeps [`RevealPayload::NotKept`] for a replay
/// of the request, never the value.
#[derive(Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum RevealPayload {
    /// The value, as the message writes it.
    Shown {
        /// The value.
        value: String,
    },
    /// The message is gone, or holds no value at that place any more.
    Gone,
    /// A replay: the value was answered once and not kept; tap again.
    NotKept,
}

impl std::fmt::Debug for RevealPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Shown { .. } => "Shown(a held value)",
            Self::Gone => "Gone",
            Self::NotKept => "NotKept",
        })
    }
}

/// `POST /api/act/device`: the phone saying "wake me here", sent by the page with its device's
/// signature, since the Android portal's native code holds no key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Registering {
    /// Which push service the token is for.
    pub platform: crate::devices::Platform,
    /// The token, one line.
    pub token: String,
}

/// `true`, the one value a yes takes on the wire, so `{"whole": false}` is no request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(type = "true")]
pub struct Yes;

impl<'de> Deserialize<'de> for Yes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if bool::deserialize(deserializer)? {
            Ok(Yes)
        } else {
            Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Bool(false),
                &"true",
            ))
        }
    }
}

/// One posted literal refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct RefusedObject {
    /// Its name, or `""` when the bytes were not an object.
    pub hash: String,
    /// The server's sentence.
    pub reason: String,
    /// Whether posting it again can ever be taken.
    pub kind: RefusalKind,
}

/// What a replica may do with a refused object: a sum, because the tab branches on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RefusalKind {
    /// Never taken as it is: held and named, and everything resting on it with it.
    Final,
    /// Taken once its device is trusted and it is signed: kept, and posted again then.
    Trust,
}

/// What `POST /api/objects` did. The lists are always present: a key that
/// appears conditionally is read as absent when it means none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ReceiptPayload {
    /// Every head after the post, sorted.
    pub heads: Vec<String>,
    /// Adopted, parents first.
    pub adopted: Vec<String>,
    /// Already held; what makes a re-post free.
    pub already: Vec<String>,
    /// Refused, in arrival order.
    pub refused: Vec<RefusedObject>,
    /// How the box read the post's `since`.
    pub since: SinceRead,
    /// The frontier the poster holds once the box took what it took: what it held at `since`
    /// and everything adopted or already held, with all they rest on.
    pub frontier: Frontier,
}

/// `GET /api/portal`: what the Android portal reads from a box before it keeps it as its box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct PortalPayload {
    /// How the box wakes the phone.
    pub push: PortalPush,
}

/// How a box wakes the portal's phone, as [`crate::portal::Portal`] reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PortalPush {
    /// No push: the portal registers with no push service.
    None,
    /// Firebase: the portal registers with Play Services against `sender`.
    Fcm {
        /// The Firebase project's number.
        sender: String,
    },
}

impl From<&crate::portal::Portal> for PortalPayload {
    fn from(portal: &crate::portal::Portal) -> Self {
        let push = match portal {
            crate::portal::Portal::Quiet => PortalPush::None,
            crate::portal::Portal::Fcm(sender) => PortalPush::Fcm {
                sender: sender.as_str().to_owned(),
            },
        };
        Self { push }
    }
}

/// `GET /api/about`: the version and the revision this build is, and what the version
/// completed.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct AboutPayload {
    /// The release this build was stamped with, or `dev`.
    pub version: String,
    /// The short git revision, or `dev` outside the flake.
    pub build: String,
    /// The todos this version completed (`app/release.toml`), first named first: what
    /// an update brought.
    pub completes: Vec<Shipped>,
}

/// One todo a version completed.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Shipped {
    /// The todo's id, on the release plan's chain.
    pub todo: String,
    /// Its row and title as that chain reads them now, or `null` where the box does not
    /// serve the chain or the chain does not hold the todo: a release file names ids alone.
    pub held: Option<HeldTodo>,
}

/// `GET /api/releases`: the releases the project chain reads, each with its items.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ReleasesPayload {
    /// The chain the releases are todos of.
    pub chain: String,
    /// The `major.minor` it reads now (the last completed release's), which the next train's
    /// version starts from.
    pub reads: String,
    /// Each declared release, in the order they ship; none where the chain is not served.
    pub releases: Vec<ReleaseRow>,
}

/// One release: its todo, its version, where it stands in the plan, and its items.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ReleaseRow {
    /// The release's todo.
    pub todo: String,
    /// Its row and title, or `null` while the plan declares a todo the chain does not hold yet.
    pub held: Option<HeldTodo>,
    /// The `major.minor` its completion moves the version to.
    pub version: String,
    /// What became of it: completed once is shipped.
    pub lot: crate::release::Lot,
    /// Where it stands in the plan.
    pub place: crate::release::Place,
    /// Every todo its spec has named, in id order.
    pub items: Vec<ReleaseItem>,
}

/// One item of a release.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ReleaseItem {
    /// The item's todo.
    pub todo: String,
    /// Its row and title, or `null` where the spec names a todo the chain does not hold.
    pub held: Option<HeldTodo>,
    /// What became of it: completed once is done.
    pub lot: crate::release::Lot,
    /// Whether the release's spec names it now.
    pub named: crate::release::Named,
}

/// A todo the chain holds, as a release's row reads it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct HeldTodo {
    /// Its state and fulfillment now.
    pub row: Neighbour,
    /// Its action line as plain text.
    pub title: String,
}

/// `GET /api/me`: who the gate says this login is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MePayload {
    /// The Tailscale login, `""` for the local shell.
    pub login: String,
    /// The actor it writes as on this chain, or `null` for a viewer (or a
    /// mapping this chain's roster does not know).
    pub actor: Option<String>,
    /// Every chain served, in the instance file's order.
    pub chains: Vec<String>,
    /// This deployment's categories, in its order: what a page offers and sorts by, so no page
    /// spells the vocabulary itself.
    pub categories: Vec<String>,
    /// The owner's zone by name (`America/New_York`): where a day is read and a time printed,
    /// never the device's zone.
    pub zone: String,
}

/// `POST /api/author`: the authoring without the append, the one thing a
/// replica cannot do for itself; it seals the event on its own head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct AuthoredPayload {
    /// The todo.
    pub todo: String,
    /// The event's canonical print.
    pub event: String,
    /// A subtodo's parent revision, sealed after `event`.
    pub revision: Option<String>,
}

/// Every refusal: the server's own sentence, shown verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ErrorPayload {
    /// What was wrong.
    pub error: String,
}

/// One field a proposal would change, in prose for somebody deciding, where
/// [`FieldChange`] carries the canonical print as evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ProposedChange {
    /// The name a person uses.
    pub field: String,
    /// What the chain says today, `""` when absent.
    pub before: String,
    /// What accepting makes it.
    pub after: String,
}

/// One event a changeset builds, as a person reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ProposedObject {
    /// Its place in the changeset, from 1.
    pub position: usize,
    /// What it does, the row read as a sentence.
    #[serde(flatten)]
    #[ts(flatten)]
    pub act: ObjectAct,
}

/// What one proposed event does to a todo, carrying what its kind of act shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "act", rename_all = "lowercase")]
pub enum ObjectAct {
    /// A todo's first content.
    Added {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// Every field it writes.
        changes: Vec<ProposedChange>,
    },
    /// A todo's content reworded.
    Edited {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// What it changes.
        changes: Vec<ProposedChange>,
    },
    /// A todo's content with only its price and its reason moved.
    Repriced {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// What it changes.
        changes: Vec<ProposedChange>,
    },
    /// A todo registered.
    Created {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// The event's note.
        note: String,
    },
    /// A todo done.
    Completed {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// The event's note.
        note: String,
    },
    /// A todo given up.
    Cancelled {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// The event's note.
        note: String,
    },
    /// A todo opened again.
    Reopened {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// The event's note.
        note: String,
    },
    /// A pass at a recurring todo.
    Tended {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// The event's note.
        note: String,
    },
    /// A todo's spec revised.
    Respecified {
        /// The todo it is about.
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        /// The event's note.
        note: String,
    },
}

impl ObjectAct {
    /// The word the row's sentence starts from, as the wire tags it.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Self::Added { .. } => "added",
            Self::Edited { .. } => "edited",
            Self::Repriced { .. } => "repriced",
            Self::Created { .. } => "created",
            Self::Completed { .. } => "completed",
            Self::Cancelled { .. } => "cancelled",
            Self::Reopened { .. } => "reopened",
            Self::Tended { .. } => "tended",
            Self::Respecified { .. } => "respecified",
        }
    }

    /// The todo it acts on.
    #[must_use]
    pub fn todo(&self) -> &ProposedTodo {
        match self {
            Self::Added { todo, .. }
            | Self::Edited { todo, .. }
            | Self::Repriced { todo, .. }
            | Self::Created { todo, .. }
            | Self::Completed { todo, .. }
            | Self::Cancelled { todo, .. }
            | Self::Reopened { todo, .. }
            | Self::Tended { todo, .. }
            | Self::Respecified { todo, .. } => todo,
        }
    }

    /// What a content write changes; none for a lifecycle event.
    #[must_use]
    pub fn changes(&self) -> &[ProposedChange] {
        match self {
            Self::Added { changes, .. }
            | Self::Edited { changes, .. }
            | Self::Repriced { changes, .. } => changes,
            Self::Created { .. }
            | Self::Completed { .. }
            | Self::Cancelled { .. }
            | Self::Reopened { .. }
            | Self::Tended { .. }
            | Self::Respecified { .. } => &[],
        }
    }

    /// A lifecycle event's note; none for a content write.
    #[must_use]
    pub fn note(&self) -> Option<&str> {
        match self {
            Self::Created { note, .. }
            | Self::Completed { note, .. }
            | Self::Cancelled { note, .. }
            | Self::Reopened { note, .. }
            | Self::Tended { note, .. }
            | Self::Respecified { note, .. } => Some(note),
            Self::Added { .. } | Self::Edited { .. } | Self::Repriced { .. } => None,
        }
    }
}

/// The todo an object acts on, as the row shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ProposedTodo {
    /// The todo's id.
    pub todo: String,
    /// The todo's action line in prose, as of this object; `""` while nothing has authored one.
    pub body: String,
    /// The instant the event claims.
    pub at: String,
    /// Who wrote it.
    pub actor: String,
    /// Its todo has no content on the chain: it waits on another card.
    pub pending_todo: bool,
}

/// Who said a turn, and what only that speaker's turn carries. The route's match over `Turn`
/// has no wildcard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "who", rename_all = "lowercase")]
pub enum Speaker {
    /// What the owner typed.
    Person {
        /// What the turn points at.
        #[ts(type = "Array<string>")]
        references: Vec<Reference>,
        /// Its photos and PDFs.
        attachments: Vec<AttachmentRow>,
        /// No round has read it yet.
        queued: bool,
    },
    /// What the model answered.
    Model {
        /// The answer split at top-level blank lines.
        blocks: Vec<BlockRow>,
    },
    /// What a verb answered.
    Verb {
        /// The verb's name.
        verb: String,
        /// How the call ended.
        outcome: VerbOutcome,
    },
    /// What the owner did rather than said, as the gate stamped it.
    Act {
        /// What the act points at.
        #[ts(type = "Array<string>")]
        references: Vec<Reference>,
    },
    /// What the box said rather than the owner: a note of its own state (a card it filed, a
    /// restart, a grant stored) or another's words it relays (a sign-in's link and code).
    Box {
        /// What the note points at.
        #[ts(type = "Array<string>")]
        references: Vec<Reference>,
    },
}

/// Which attempt at its round a turn belongs to; a rewind sets turns aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Attempt {
    /// On the live path.
    Current,
    /// Set aside by a regenerate or an edit.
    Earlier,
}

/// A photo or PDF on a person's turn, and what an upload answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct AttachmentRow {
    /// Its name in the upload store.
    pub id: String,
    /// `application/pdf` or an image type.
    pub media_type: String,
    /// The name it was picked with; `""` for a photo, which the phone redraws.
    pub name: String,
    /// Where a private file is kept in the owner's Drive; `""` for one on the box.
    pub private: String,
}

/// What an upload answers: kept, or held back until the owner says to keep it private.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Uploaded {
    /// Kept, on the box or, when asked, in the owner's Drive.
    Kept {
        /// The file.
        file: AttachmentRow,
    },
    /// Not kept: its text names an identity number, so it goes to the owner's Drive or
    /// nowhere, and only on his say.
    Sensitive {
        /// Why, as a sentence.
        why: String,
        /// Where it would be kept.
        folder: String,
    },
}

impl From<&crate::attachment::Attachment> for AttachmentRow {
    fn from(attachment: &crate::attachment::Attachment) -> Self {
        Self {
            id: attachment.id.to_string(),
            media_type: attachment.media_type.as_str().to_owned(),
            name: attachment.name.clone().unwrap_or_default(),
            private: attachment
                .private
                .as_ref()
                .map(|copy| copy.path.to_string())
                .unwrap_or_default(),
        }
    }
}

/// One turn, for a page that draws a speaker, text and a marker, tagged by who said it so a row
/// holds only its speaker's fields. `text` is the least trusted string served and is drawn as
/// text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct TurnRow {
    /// Its absolute index; an `?about=` window has gaps.
    pub index: usize,
    /// What was said, or what the verb answered.
    pub text: String,
    /// When it was said, as stored beside the turn.
    pub at: String,
    /// Which attempt; `index` still names an earlier one's turn.
    pub attempt: Attempt,
    /// Why its words are not here, when they are not: `text` is then the box's sentence for it,
    /// drawn in their place and never as anybody's words.
    pub gone: Option<TurnGone>,
    /// Who said it, with what only that speaker's turn carries.
    #[serde(flatten)]
    #[ts(flatten)]
    pub who: Speaker,
}

/// Why a turn's words are not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "why", rename_all = "lowercase")]
pub enum TurnGone {
    /// Erased on a tap.
    Asked {
        /// Who erased them.
        by: String,
    },
    /// Erased when a fact their remembering wrote was forgotten.
    Forgot {
        /// The fact.
        fact: crate::secretary::memory::FactId,
    },
    /// Their key is not here: lost in a restore, or never given to this replica.
    Missing,
}

impl From<&crate::secretary::words::Gone> for TurnGone {
    fn from(gone: &crate::secretary::words::Gone) -> Self {
        use crate::secretary::erasure::Erasure;
        use crate::secretary::words::Gone;
        match gone {
            Gone::Erased {
                erasure: Erasure::Asked { by },
            } => Self::Asked {
                by: by.as_str().to_owned(),
            },
            Gone::Erased {
                erasure: Erasure::Forgot { fact },
            } => Self::Forgot { fact: *fact },
            Gone::Missing => Self::Missing,
        }
    }
}

/// One block of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BlockRow {
    /// Its typst with the blank line that ends it.
    pub source: String,
    /// Its words as typst's parser reads them.
    pub plain: String,
    /// What it is.
    pub kind: crate::secretary::answer::Kind,
    /// Typst's HTML, computed on read; `null` when it names `html`, fails, or trips the wire.
    pub html: Option<String>,
}

/// How a verb's call ended, as its turn shows it: the stored outcome, its fact read as a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "is", rename_all = "lowercase")]
pub enum VerbOutcome {
    /// The verb answered.
    Answered,
    /// The verb refused.
    Refused,
    /// `remember` wrote a fact.
    Remembered {
        /// The fact, as its receipt shows it.
        receipt: Receipt,
    },
}

/// One fact a round remembered, as its receipt shows it. Its words are read from the memory by
/// its number, never from the verb's answer, so a forgotten fact's receipt has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Receipt {
    /// Still in the memory.
    Standing {
        /// Its number, which Forget names.
        fact: crate::secretary::memory::FactId,
        /// Its words as the memory holds them, model text drawn as text.
        text: String,
    },
    /// Forgotten since: its words are gone.
    Forgotten {
        /// Its number.
        fact: crate::secretary::memory::FactId,
    },
    /// The memory would not read, or holds no words for it that are not forgotten.
    Unread {
        /// Its number, which Forget still names.
        fact: crate::secretary::memory::FactId,
    },
}

/// `GET /api/secretary/memory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MemoryPayload {
    /// Oldest first.
    pub facts: Vec<FactRow>,
    /// How many may stand at once.
    pub cap: u32,
    /// The numbers of the facts forgotten, oldest first: a receipt naming one says so.
    pub forgotten: Vec<crate::secretary::memory::FactId>,
    /// The words the memory's text store keeps for remember cards, by the name a card gives
    /// them: a card whose name is not here has had its words deleted.
    pub cards: Vec<CardWords>,
}

/// A remember card's words as the memory's text store keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardWords {
    /// The name the card gives them, 64 hex.
    pub text: String,
    /// The fact, model text drawn as text.
    pub fact: String,
}

/// One standing fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FactRow {
    /// Its number, which the forget route names.
    pub id: crate::secretary::memory::FactId,
    /// When it was remembered.
    pub at: String,
    /// The fact, model text drawn as text.
    pub fact: String,
    /// What it is about.
    pub kind: crate::secretary::memory::Kind,
    /// Where it was said.
    pub said: SaidOn,
}

/// Where a fact was said: a round's opening turn, or a tapped card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SaidOn {
    /// A round the owner opened.
    Turn {
        /// The conversation.
        conversation: String,
        /// The opening turn.
        turn: u32,
    },
    /// A remember card the owner tapped.
    Tapped {
        /// The conversation.
        conversation: String,
        /// The proposal.
        proposal: u32,
    },
}

/// What became of a proposal. `outcome` beside it stays free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ProposalState {
    /// Nobody has answered it.
    Standing,
    /// The far end took it.
    Sent,
    /// A tap failed and nothing happened: Try again or Dismiss.
    Failed,
    /// A tap failed and it may have happened: never performed again until checked.
    Unknown,
    /// A send that may have gone, which the owner has checked.
    Checked,
    /// The owner said no.
    Dismissed,
    /// A later draft replaced it unseen, so it is not the owner's no.
    Superseded,
    /// The box or the secretary took it back because nothing needs it any more; `outcome` says
    /// why. Not the owner's no.
    Withdrawn,
    /// A changeset's acts are on the chain; nothing left the box.
    Applied,
    /// An act's device-code flow is running; a second tap is refused.
    Started,
    /// An act ran to completion.
    Acted,
    /// Its responses read as no state (a check of nothing): shown with why, never acted on.
    Unreadable,
}

/// What the box keeps as a connection's cache, which removing it may delete too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Cached {
    /// Nothing.
    Nothing,
    /// A mail account's mail, which syncing again rebuilds.
    Mail,
}

/// What a proposal would do. A tagged union: the acts share nothing but
/// their reasons, and a record of blank halves would be branched on anyway.
/// Each act's `sentence` is `Act::sentence`, printed once by the box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ProposedAct {
    /// A message to send, whole.
    Email {
        /// The address it is sent as.
        account: String,
        /// The recipients.
        to: Vec<String>,
        /// The copies.
        cc: Vec<String>,
        /// The subject, verbatim.
        subject: String,
        /// The body, verbatim.
        body: String,
        /// Every file it carries.
        attachments: Vec<CardFile>,
        /// The owner's held values its body carries.
        held: HeldIn,
    },
    /// A stored message to forward around a note.
    Forward {
        /// The address it is sent as.
        account: String,
        /// The recipients.
        to: Vec<String>,
        /// The copies.
        cc: Vec<String>,
        /// The subject, verbatim.
        subject: String,
        /// The note above the original, verbatim.
        note: String,
        /// Every file it carries: the original's attachments inline, or its `.eml`.
        attachments: Vec<CardFile>,
        /// The original's sender, subject and date, as one line.
        original: String,
        /// How the original travels.
        style: crate::secretary::store::Style,
        /// Each held value in the original's text, as "SSN ending 3456".
        held: Vec<String>,
    },
    /// Chain acts, drawn as the same diff rows a triage batch is.
    Changes {
        /// The chain.
        chain: String,
        /// One row per act, in order.
        changes: Vec<ProposedObject>,
        /// Why an act would no longer build, `""` while it would.
        problem: String,
    },
    /// A device-code sign-in the box performs itself.
    Auth {
        /// The account, verbatim.
        account: String,
        /// The imperative.
        sentence: String,
    },
    /// A Microsoft account's consent to send, the device-code sign-in for SMTP alone.
    #[serde(rename = "auth_send")]
    AuthSend {
        /// The account, verbatim.
        account: String,
        /// The imperative.
        sentence: String,
    },
    /// A declared secret the box asks for. No value field: the card's own
    /// password field sends it in the tap's body.
    Secret {
        /// What the owner calls it ("the Phala key"); its stored name stays on the box.
        label: String,
        /// Where its value is found, for a secret a template knows.
        found: Option<String>,
        /// The page that issues it, for a secret a template knows.
        link: Option<String>,
    },
    /// A web request; `target` is exactly what would leave the box.
    Web {
        /// Which of the two it asks.
        request: crate::web::WebKind,
        /// The URL or the query.
        target: String,
        /// The host.
        host: String,
        /// The drafted "Always allow" rule, `""` for none.
        always: String,
        /// The imperative.
        sentence: String,
    },
    /// A change to a repository; the tap pushes exactly `patch`.
    Patch {
        /// `owner/name`.
        repo: String,
        /// Who can read it once pushed.
        visibility: crate::repos::Visibility,
        /// The commit it was written against.
        base: String,
        /// The commit message.
        message: String,
        /// Every file it touches.
        files: Vec<String>,
        /// The unified diff, verbatim.
        patch: String,
        /// The imperative.
        sentence: String,
    },
    /// A document to read, redacted, into the conversation.
    Drive {
        /// The whole Drive path.
        path: String,
        /// Its name.
        name: String,
        /// Its folder, which "Always allow" would name.
        folder: String,
        /// Its size in words, `""` when unknown.
        size: String,
        /// The imperative.
        sentence: String,
    },
    /// A coding session; the tap fires the routine with exactly `task`.
    #[serde(rename = "coding_session")]
    CodingSession {
        /// The task, verbatim.
        task: String,
        /// The imperative.
        sentence: String,
    },
    /// A device asking to be trusted; the tap is endorsed by a device already trusted.
    Device {
        /// Whom the key would write as.
        actor: String,
        /// The device's public key, 64 hex.
        key: String,
        /// The imperative.
        sentence: String,
    },
    /// Sign in with the Claude subscription, which the box runs `claude setup-token` for.
    #[serde(rename = "claude_sign_in")]
    ClaudeSignIn {
        /// The imperative.
        sentence: String,
    },
    /// Unlock the secret store; the tap asks it again.
    #[serde(rename = "unlock_store")]
    UnlockStore {
        /// The imperative.
        sentence: String,
    },
    /// Erase words a forgetting could not prove were the fact's copies.
    Erase {
        /// The imperative.
        sentence: String,
    },
    /// The code an open sign-in waits for. No code field: the card's own password field sends
    /// it in the tap's body.
    #[serde(rename = "sign_in_code")]
    SignInCode {
        /// The page the owner signs in on, as the box read it off the CLI.
        url: String,
        /// The imperative.
        sentence: String,
    },
    /// The box's Google Cloud client, asked for whole: the card says how to make it and the
    /// address it must list, and its password field takes the client's downloaded file.
    #[serde(rename = "google_client")]
    GoogleClient {
        /// The box's callback, which the client must list as an authorized redirect URI.
        redirect: String,
        /// How to make the client, one step a line.
        steps: Vec<String>,
        /// The imperative.
        sentence: String,
    },
    /// A sign-in with Google for one account.
    #[serde(rename = "google_sign_in")]
    GoogleSignIn {
        /// The account.
        account: String,
        /// What it grants, in words, one part a line.
        parts: Vec<String>,
        /// While the sign-in runs, the box's own path that sends the browser on to Google's
        /// consent page; `null` otherwise.
        consent: Option<String>,
        /// The imperative.
        sentence: String,
    },
    /// An event to write into a Google account's own calendar.
    #[serde(rename = "calendar_event")]
    CalendarEvent {
        /// The account.
        account: String,
        /// The event, in words.
        event: String,
        /// The imperative.
        sentence: String,
    },
    /// A connection to write into the instance file.
    Connect {
        /// What the owner calls it: `your Work calendar`, an address, a folder.
        connection: String,
        /// What it writes, in words.
        writes: String,
        /// Each step it will ask for once written, in order, in words.
        steps: Vec<String>,
        /// The origins its plugin may reach from the start; empty for a connection of no plugin.
        reach: Vec<String>,
        /// The imperative.
        sentence: String,
    },
    /// A connection to take out of the instance file.
    Disconnect {
        /// What the owner calls it.
        connection: String,
        /// What goes, in words.
        writes: String,
        /// The secrets its steps stored, in the owner's words: the removal deletes them.
        forgets: Vec<String>,
        /// What the box keeps as its cache, which its wider yes also deletes.
        cached: Cached,
        /// The imperative.
        sentence: String,
    },
    /// One more origin a plugin may reach; only the origin, never the link it was read from.
    Grant {
        /// The plugin.
        plugin: String,
        /// The origin.
        origin: String,
        /// The imperative.
        sentence: String,
    },
    /// One setting of `[models]` or `[[providers]]` changed, from what the card read to what it
    /// writes, each in the owner's words.
    Configure {
        /// What the owner calls the setting: `Chat`, `Reserve`, a provider.
        setting: String,
        /// What it read when the card was filed.
        from: String,
        /// What the tap writes.
        to: String,
        /// The imperative.
        sentence: String,
    },
    /// A fact to remember from a round that read something not the owner's own.
    Remember {
        /// The fact's words, read from the memory's text store, or gone.
        words: RememberWords,
        /// What the fact is about.
        about: crate::secretary::memory::Kind,
        /// The imperative, or what became of the words.
        sentence: String,
    },
    /// A decision to record; the tap records exactly these fields as the owner's.
    Decision {
        /// The decision's id.
        id: String,
        /// Its title.
        title: String,
        /// Its body, Typst markup as written.
        body: String,
        /// Why.
        rationale: String,
        /// The decisions it supersedes, by id.
        supersedes: Vec<String>,
        /// The imperative.
        sentence: String,
    },
}

/// A remember card's words: the card names them and the memory's text store keeps them, so a
/// card dismissed, or whose fact was forgotten, has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum RememberWords {
    /// Kept: the card shows them.
    Kept {
        /// The fact.
        fact: String,
    },
    /// Deleted: the card was dismissed, or the fact it became forgotten.
    Gone {},
    /// Not read here: the reading was given no memory, so it cannot say.
    Unread {},
}

/// Who asked for a proposal: the page tags a card by it and folds the setup cards into one Setup
/// row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Asker {
    /// The secretary, whose cards every card before plugins was.
    Secretary {},
    /// The triage agent, which reads mail anyone can send.
    Triage {},
    /// An `actor` plugin, by its name.
    Plugin {
        /// The plugin's name.
        name: String,
    },
    /// The box itself, with no model asked: a secret the deployment declares and nothing stores,
    /// asked for in the setup conversation.
    Setup {},
    /// The box itself, with no model asked, for anything else: the store to unlock, a sign-in, an
    /// origin to allow, what was said around a forgotten fact.
    Box {},
}

/// A proposal's whole presentation, decided by the box, so the page knows no
/// kinds. Nothing is clamped: the card is the confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Card {
    /// The head line.
    pub title: String,
    /// Who asked for it.
    pub by: Asker,
    /// What it would do, `""` when the fields say it.
    pub sentence: String,
    /// Every field, whole.
    pub fields: Vec<CardField>,
    /// A verbatim block under the fields, `""` for none.
    pub body: String,
    /// A changeset's diff rows.
    pub rows: Vec<ProposedObject>,
    /// An email's attachments, every one, each with a way to open it.
    pub files: Vec<CardFile>,
    /// Why it cannot be performed as it stands; a card with one offers no yes.
    pub problem: String,
    /// The yes answers, the first the main one.
    pub answers: Vec<CardAnswer>,
    /// What to check after a tap that may have gone through.
    pub uncertain: String,
    /// What a login that may not answer sees.
    pub readonly: String,
    /// A page the owner opens to do what the card asks, `null` for none: only ever a template's
    /// own https link or a path on the box itself, so no model's words are drawn as a button.
    pub link: Option<CardLink>,
}

/// A link a card draws as a button, from the box's own table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CardLink {
    /// What the button says.
    pub label: String,
    /// Where it goes: an https page, or a path on the box itself.
    pub href: String,
}

/// The held values a proposed email's body carries, as its card names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum HeldIn {
    /// Each value, in the order the body names it.
    Read {
        /// One per placeholder.
        values: Vec<HeldFrom>,
    },
    /// A placeholder whose handle does not decode; the tap refuses the send.
    Unreadable {
        /// The box's words.
        why: String,
    },
}

/// One held value a proposed email carries: what it is, and the message it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct HeldFrom {
    /// The kind and hint: `SSN ending 3456`.
    pub named: String,
    /// The message, in the box's words: its sender, subject and date, or that it is gone.
    pub from: String,
}

/// One file a proposed email carries, as the card shows it before the tap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CardFile {
    /// What the recipient sees it called.
    pub name: String,
    /// Its `type/subtype`.
    pub media: String,
    /// Its size, for a person ("1.2 MB").
    pub size: String,
    /// The SHA-256 of its bytes, hex: what the tap sends is exactly these.
    pub sha256: String,
    /// Where it is read from, which is where the card opens it.
    pub from: FileFrom,
}

/// Where an attached file comes from, as the card links to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FileFrom {
    /// An attachment of a message, opened on the message's page.
    Mail {
        /// The message's notmuch id.
        message: String,
    },
    /// A file in the owner's Drive, named by its path.
    Drive {
        /// Its full path.
        path: String,
    },
    /// A photo or PDF the owner uploaded, opened as itself.
    Upload {
        /// Its id in the upload store.
        id: String,
    },
    /// A file the owner allowed once, the copy read into this conversation.
    Kept {},
    /// A file of one of the owner's repositories at a commit, as the box kept it.
    Repo {
        /// `owner/name`.
        repo: String,
        /// The file's path in it.
        path: String,
        /// The commit it was read at.
        at: String,
    },
}

/// One labelled field of a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CardField {
    /// What it is.
    pub label: String,
    /// Its value, verbatim.
    pub value: String,
}

/// One way to answer a card yes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CardAnswer {
    /// `send`, `confirm`, `run`, …; the button's class is `act-<verb>`.
    pub verb: String,
    /// The button.
    pub label: String,
    /// What the app says once done.
    pub done: String,
    /// What the answer asks of the owner before its tap, and what the tap carries.
    #[serde(flatten)]
    #[ts(flatten)]
    pub asks: Asks,
}

/// What a card's answer asks before its tap: nothing, a confirm step (whose tap carries the
/// owner's note, `always`, or this device's signature of the card), or a typed value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "asks", rename_all = "lowercase")]
pub enum Asks {
    /// Nothing: the button taps at once.
    Now,
    /// A confirm step; the tap carries the owner's note.
    Confirm {
        /// The confirm step's question.
        question: String,
        /// The confirm step's yes.
        yes: String,
    },
    /// A confirm step; the tap carries `always`, the card's wider yes.
    Always {
        /// The confirm step's question.
        question: String,
        /// The confirm step's yes.
        yes: String,
    },
    /// A confirm step; the tap carries this device's signature of the card, a yes only a
    /// trusted device can give.
    Endorse {
        /// The confirm step's question.
        question: String,
        /// The confirm step's yes.
        yes: String,
    },
    /// A typed value, sent only in the tap's body, with no confirm step.
    Value {
        /// The field's label.
        input: String,
    },
}

/// Proposals drawn against the todo chains, in the order their indices were asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct DrawnPayload {
    /// One row per index asked.
    pub proposals: Vec<ProposalRow>,
}

/// One proposal, every field whole: the tap names an index, so what is
/// shown is exactly what is sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ProposalRow {
    /// Its index, what a tap names.
    pub index: usize,
    /// What it would do.
    pub act: ProposedAct,
    /// How it is drawn and answered.
    pub card: Card,
    /// Why it was proposed.
    pub why: String,
    /// What it rests on, never empty.
    #[ts(type = "Array<string>")]
    pub sources: Vec<Reference>,
    /// When.
    pub at: String,
    /// Where it stands.
    pub state: ProposalState,
    /// The transport's words, the recipients, or the owner's reason; `""` while standing.
    pub outcome: String,
    /// How many times a tap performed it.
    pub tries: u32,
    /// Turns standing before it; drawn after turn `turn - 1`, never renumbered.
    pub turn: usize,
    /// The coding session it started.
    pub coding: Option<CodingSession>,
    /// While its act runs, what the box said it waits for (a sign-in's link and code), drawn on
    /// the card where it was asked; `None` once it ends, or for an act that says nothing.
    pub announced: Option<String>,
    /// The card of the same conversation it replaces, `None` for a first asking.
    pub revises: Option<Revises>,
}

/// The card a proposal replaces, and which of its own rows that card already asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub struct Revises {
    /// The replaced card's index in the same conversation.
    pub card: usize,
    /// How many changeset rows, from the first, the replaced card already asked for; every row
    /// after them is new. `0` where either card is not a changeset.
    pub carried: usize,
}

/// One proposal in the inbox prodrome, as the Inbox lists it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct InboxProposal {
    /// The object's id.
    pub id: String,
    /// Where it was asked.
    #[ts(type = "string")]
    pub asked: crate::reference::ProposalRef,
    /// The todo it serves.
    #[ts(type = "string | null")]
    pub serves: Option<crate::reference::TodoRef>,
    /// One line, for a list.
    pub headline: String,
    /// Who asked for it.
    pub by: Asker,
    /// Why it was proposed.
    pub why: String,
    /// Where it stands.
    pub state: InboxState,
    /// Its fulfillment now.
    pub value: Option<Fulfillment>,
    /// When it was asked.
    pub at: String,
}

/// Where a proposal stands, as the Inbox lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum InboxState {
    /// Waiting on the owner.
    Open {},
    /// Performed or checked, at `at`.
    Completed {
        /// When.
        at: String,
    },
    /// Dismissed, at `at`.
    Cancelled {
        /// When.
        at: String,
    },
    /// Its state cannot be read: never waiting, since nothing acts on it.
    Unreadable {
        /// Why.
        reason: String,
    },
}

/// `GET /api/waiting`: what the Inbox badge shows, read on the box so a tab need not hold the
/// conversations to show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct WaitingPayload {
    /// The cards the Inbox draws: open proposals, open decisions (one per batch they name), and
    /// proposal lines no record places.
    pub count: usize,
}

/// `GET /api/inbox`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct InboxProposalsPayload {
    /// The instant read at.
    pub at: String,
    /// Oldest first.
    pub proposals: Vec<InboxProposal>,
    /// Every chain the proposals were priced without, held as a failure.
    pub failed: Vec<FailedChain>,
}

/// A coding session as a page draws it: the stored `coding::Session`
/// flattened, with whether it has been handed over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum CodingSession {
    /// Running.
    Fired {
        /// The session.
        id: String,
        /// Where it can be watched.
        url: String,
        /// When it was fired.
        at: String,
    },
    /// Its branch came back with a report.
    Reported {
        /// The session.
        id: String,
        /// The `claude/` branch.
        branch: String,
        /// `docs/reports/<slug>.md`.
        path: String,
        /// The report, up to its cut.
        report: String,
        /// Every file the branch changes.
        stat: Vec<CodingFile>,
        /// The handed-over todo as `todo:<chain>/<id>`, or `""`.
        handed: String,
    },
    /// Its branch is on the default branch.
    Merged {
        /// The session.
        id: String,
        /// The tip as merged.
        sha: String,
    },
    /// Let go.
    Dropped {
        /// The session.
        id: String,
        /// The owner's words.
        reason: String,
    },
}

/// One file of a session's diff stat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CodingFile {
    /// The file.
    pub path: String,
    /// Lines added; `null` for a binary file.
    pub added: Option<u32>,
    /// Lines removed; `null` for a binary file.
    pub removed: Option<u32>,
}

/// One SSE frame of `GET …/stream`. A delta and a working verb are never
/// written down; `Turn` says the transcript grew, refetch it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Frame {
    /// The answering state changed.
    State {
        /// The new state.
        state: crate::secretary::store::ConversationState,
    },
    /// A fragment of the model's reply, verbatim, drawn as text.
    Delta {
        /// The fragment.
        text: String,
    },
    /// Something was appended; cards come only from the refetch.
    Turn,
    /// A verb is about to run.
    Working {
        /// The verb's wire name.
        verb: String,
    },
    /// A block of the streaming answer closed; the page asks for its render by
    /// `sha`, never by text, so it cannot make the box render other words.
    Block {
        /// Its place, from 0.
        n: usize,
        /// sha256 of `source`, hex.
        sha: String,
        /// Its typst.
        source: String,
    },
}

/// `GET …/blocks/<sha>`: typst's HTML for one block of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BlockRender {
    /// The block's HTML, through the render tripwire.
    pub html: String,
    /// The key it is kept under ([`crate::rendering::key`] of the answer function at its source).
    pub key: String,
}

/// `GET /api/secretary/conversations/<id>`, windowed by `?before=&limit=`
/// and filtered to rounds touching `?about=`. Every index is absolute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ConversationPayload {
    /// Its id.
    pub id: String,
    /// What it is about.
    pub title: String,
    /// Which model is behind it.
    pub model: String,
    /// When it was opened.
    pub opened: String,
    /// The window.
    pub turns: Vec<TurnRow>,
    /// Every proposal whose round was kept, never windowed.
    pub proposals: Vec<ProposalRow>,
    /// Every proposal line the inbox's reading cannot place, never windowed.
    pub unplaced: Vec<UnplacedProposal>,
    /// The page polls while this is not `idle`.
    pub state: crate::secretary::store::ConversationState,
    /// The earliest index carried, `total` when empty.
    pub first: usize,
    /// How many turns the paged sequence holds.
    pub total: usize,
    /// The question the last round may be asked again from, as `Stored::rewind` would take it.
    pub again: Option<usize>,
}

/// A proposal line the inbox's reading cannot place: drawn with why, and nothing acts on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct UnplacedProposal {
    /// Its index among the conversation's proposals, what a tap would name.
    pub index: usize,
    /// Why it has no card.
    pub why: crate::secretary::store::Unheld,
}

/// One row of `GET /api/secretary/conversations`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ConversationRow {
    /// Its id.
    pub id: String,
    /// What it is about.
    pub title: String,
    /// What it is for.
    pub kind: crate::secretary::store::Kind,
    /// When it was opened.
    pub opened: String,
    /// How many turns.
    pub turns: u32,
    /// How many proposals are standing, the number worth a badge.
    pub standing: usize,
    /// Every proposal line the inbox's reading cannot place, each a problem the Inbox draws.
    pub unplaced: Vec<UnplacedProposal>,
    /// One line of the last thing said or answered.
    pub last: String,
    /// When anything was last said, what topics sort by.
    pub last_at: String,
}

/// What a search answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SearchPayload {
    /// The query, trimmed.
    pub query: String,
    /// Newest first.
    pub hits: Vec<crate::secretary::search::Hit>,
}

/// The rounds of every conversation a topic filter shows (`secretary::stream`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct StreamPayload {
    /// The newest a page asked for, oldest first.
    pub rounds: Vec<StreamRound>,
    /// How many rounds the filter shows in all.
    pub total: usize,
}

/// One round of the stream: the question that opened it and its answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct StreamRound {
    /// `round:<conversation>/<turn>`, the target its topics are on.
    pub target: String,
    /// The conversation it was asked in.
    pub conversation: String,
    /// That conversation's heading.
    pub title: String,
    /// The index of its opening turn.
    pub turn: usize,
    /// The turn that opened it.
    pub opening: TurnRow,
    /// Its last model turn with words, absent while it is going or when it said nothing.
    pub answer: Option<TurnRow>,
    /// Its topics, each in its current spelling, sorted.
    pub topics: Vec<String>,
    /// How many more topics it may carry.
    pub room: usize,
}

/// The topics in use, the most rounds first (`topics::reading::Reading::in_use`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct TopicsPayload {
    /// Every topic on a target.
    pub topics: Vec<TopicInUse>,
}

/// One topic in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct TopicInUse {
    /// Its current spelling.
    pub topic: String,
    /// How many rounds carry it.
    pub rounds: usize,
    /// The other names concurrent renames disagree on for it, sorted; empty when none do.
    pub disputed: Vec<String>,
}

/// The conversation list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ConversationsPayload {
    /// Newest first.
    pub conversations: Vec<ConversationRow>,
}

/// What `POST …/say` answers: the conversation with the turn already on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SayPayload {
    /// The conversation.
    pub conversation: ConversationPayload,
}

/// `GET /api/overview`: what is connected, by group, and never a secret's name or value.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Overview {
    /// Every group, in `Group::ALL`'s order; one nothing is connected to holds no line.
    pub groups: Vec<OverviewGroup>,
    /// The conversation Ask opens.
    pub ask: String,
}

/// One group and what of it is connected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct OverviewGroup {
    /// Which.
    pub group: crate::connect::Group,
    /// Its lines, by key.
    pub lines: Vec<OverviewLine>,
}

/// One connection: what the owner calls it and where it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct OverviewLine {
    /// Its identity within its group, never a secret's name.
    pub key: String,
    /// What the owner calls it.
    pub label: String,
    /// One sentence of what it is, or `null`.
    pub detail: Option<String>,
    /// Where it stands.
    pub standing: crate::connect::Standing,
    /// Its open cards, each a `proposal:<conversation>/<n>`, every need's on one line only.
    pub cards: Vec<String>,
}

/// `GET /api/calendar/{date}`: a week of the owner's calendars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CalendarPayload {
    /// The first day, ISO 8601.
    pub from: String,
    /// How many days.
    pub days: u32,
    /// The days as plain text.
    pub text: String,
    /// The days as typst's HTML, or `null` where nothing rendered them.
    pub rich: Option<String>,
}

/// `Todos.verify_history` in the browser's module: SPEC §3's verify of the objects a tab holds,
/// with what predates signatures read as history rather than as a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct VerifiedPayload {
    /// The heads of the objects, derived.
    pub tips: Vec<String>,
    /// Walk order, heads first, then any object the walk never reached.
    pub objects: Vec<ObjectVerdict>,
    /// How many objects the walk reached.
    pub walked: u32,
    /// How many objects hashed to their own name.
    pub verified: u32,
    /// The genesis objects the walk ended at.
    pub genesis: Vec<String>,
    /// Every object in one causal order.
    pub linearisation: Vec<String>,
    /// What the check found.
    pub found: Found,
}

/// One object as the verifier read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ObjectVerdict {
    /// The name it claims, which its children's parents point at.
    pub hash: String,
    /// The hash of its literal.
    pub computed: String,
    /// Whether the two agree.
    #[serde(rename = "hashOk")]
    pub hash_ok: bool,
    /// Where the walk from the heads put it.
    pub walk: Walk,
    /// Whether anything is wrong with it.
    pub condition: Condition,
    /// What it holds, as the verifier read it.
    #[serde(flatten)]
    #[ts(flatten)]
    pub holds: Verified,
}

/// What a verified object holds: an envelope with no event, or an event with its todo, writer and
/// instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "holds", rename_all = "lowercase")]
pub enum Verified {
    /// No event.
    Envelope {
        /// The envelope's constructor.
        kind: String,
    },
    /// An event.
    Event {
        /// The event's constructor.
        kind: String,
        /// The todo it is about.
        todo: String,
        /// Who wrote it.
        actor: String,
        /// When, ISO 8601.
        at: String,
    },
}

/// Whether the verifier found anything wrong with one object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Condition {
    /// Nothing.
    Clean,
    /// One thing, as the verifier says it.
    Flawed {
        /// What is wrong.
        problem: String,
    },
}

impl Condition {
    /// The Prodrome's print of a verdict, which says clean with an empty sentence.
    #[must_use]
    pub fn of(problem: String) -> Self {
        if problem.is_empty() {
            Self::Clean
        } else {
            Self::Flawed { problem }
        }
    }
}

/// Where the walk from the heads put one object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Walk {
    /// Reached, at its distance from the nearest head (0 is a head).
    Reached {
        /// The distance.
        depth: u32,
    },
    /// Not reached from any head.
    Unreached,
}

/// What verifying found: every object clean and reached, or every problem in the order found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Found {
    /// Nothing wrong.
    Reached,
    /// At least one problem.
    Unreached {
        /// Each problem, as the verifier says it.
        #[ts(type = "Array<string>")]
        problems: crate::nonempty::NonEmpty<String>,
    },
}

impl Found {
    /// No problem is [`Found::Reached`]; any is [`Found::Unreached`] with all of them.
    #[must_use]
    pub fn of(problems: Vec<String>) -> Self {
        crate::nonempty::NonEmpty::of(problems)
            .map_or(Self::Reached, |problems| Self::Unreached { problems })
    }
}

/// `Todos.provenance` in the browser's module: who wrote one object carrying an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ProvenanceRow {
    /// The object.
    pub hash: String,
    /// What proves who wrote it.
    pub proof: Proof,
    /// The sentence a page shows.
    pub said: String,
}

/// What proves who wrote an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Proof {
    /// A signature by a key registered to its actor.
    Proven {
        /// The key.
        key: String,
    },
    /// Unsigned and beneath an epoch: written before this deployment signed its objects.
    Predates,
    /// Not proven, and not earlier than signatures; `said` says why.
    Unproven,
}

impl From<&crate::signatures::Provenance> for Proof {
    fn from(provenance: &crate::signatures::Provenance) -> Self {
        use crate::signatures::Provenance;
        match provenance {
            Provenance::Proven { key } => Self::Proven {
                key: key.as_str().to_owned(),
            },
            Provenance::Predates => Self::Predates,
            Provenance::Unproven(_) => Self::Unproven,
        }
    }
}

/// `signed` in the browser's module: the `Signed` it sealed for a signature the tab made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SignedObject {
    /// Its name.
    pub hash: String,
    /// Its canonical print.
    pub text: String,
}

/// [`crate::term_json::to_json`]'s shape, open: a term is a heterogeneous tree
/// that `prodrome_core::term::Term` types and nothing derives `TS` for.
const SPEC_NODE: &str = "type SpecNode = { kind: string } & Record<string, unknown>;";

const HEADER: &str = "\
// GENERATED FROM app/core/src/wire.rs BY `cargo test -p suzatary-core`.
// DO NOT EDIT: docs/ARCHITECTURE.md §2 — every wire type is declared ONCE, in
// suzatary-core, and its decoder in payload.ts is folded from the same text.

/** A `to_json` node: tagged by its Term constructor, with per-constructor
 * fields a consumer renders generically. */
export ";

/// Every wire type's TypeScript, in one file: the answers and the bodies and
/// every type they reach, each declared once, what it names before itself.
#[must_use]
pub fn typescript() -> String {
    let mut out = String::from(HEADER);
    out.push_str(SPEC_NODE);
    out.push('\n');
    for decl in declared() {
        out.push_str("\nexport ");
        out.push_str(&decl);
        if !decl.ends_with(';') {
            out.push(';');
        }
        out.push('\n');
    }
    out
}

/// The browser's decoders, `web/src/wire/generated/payload.ts`: one per type a
/// route answers and per type it reaches, folded from the declarations
/// [`typescript`] prints (`shape::decoders`).
///
/// # Errors
/// A declaration with no decoder, as [`shape::decoders`] says.
pub fn decoders() -> Result<String, shape::ShapeError> {
    let declared = declared();
    shape::decoders(
        std::iter::once(SPEC_NODE).chain(declared.iter().map(String::as_str)),
        &answer_names(),
    )
}

/// The TypeScript names of what a route answers: the roots of [`decoders`].
#[must_use]
pub fn answer_names() -> Vec<String> {
    let mut names = Names::default();
    answers(&mut names);
    names.0
}

/// Where `value`, an answer named `name`, holds a map, whose keys are data and never fields
/// ([`shape::maps`]).
///
/// # Errors
/// A declaration with no shape, as [`shape::maps`] says.
pub fn maps(
    name: &str,
    value: &serde_json::Value,
) -> Result<Vec<Vec<serde_json::Value>>, shape::ShapeError> {
    let declared = declared();
    shape::maps(
        std::iter::once(SPEC_NODE).chain(declared.iter().map(String::as_str)),
        name,
        value,
    )
}

/// ts-rs's declarations of the answers, the bodies and what they reach.
fn declared() -> Vec<String> {
    let mut declared = Declarations::default();
    answers(&mut declared);
    bodies(&mut declared);
    declared.out
}

/// What a route answers, a stream sends or the browser's module reads out: each
/// one a root of the browser's decoders, everything else they decode reached
/// from these.
fn answers(visit: &mut impl TypeVisitor) {
    // The browser's module's own answers.
    visit.visit::<VerifiedPayload>();
    visit.visit::<ProvenanceRow>();
    visit.visit::<SignedObject>();
    visit.visit::<ChainPayload>();
    visit.visit::<FoldPayload>();
    visit.visit::<TodoPayload>();
    // Named only inside `TodoPayload::diffs`'s `#[ts(type)]`.
    visit.visit::<FieldChange>();
    visit.visit::<TreePayload>();
    visit.visit::<TipPayload>();
    visit.visit::<TipFrame>();
    visit.visit::<StoreFrame>();
    visit.visit::<BriefingsPayload>();
    visit.visit::<BriefingPayload>();
    visit.visit::<CalendarPayload>();
    visit.visit::<InboxPayload>();
    visit.visit::<MailSearchPayload>();
    visit.visit::<ThreadPayload>();
    visit.visit::<WatchesPayload>();
    visit.visit::<ObjectsPayload>();
    visit.visit::<StorePayload>();
    visit.visit::<ContentPayload>();
    visit.visit::<SkeletonPayload>();
    visit.visit::<WaitingPayload>();
    visit.visit::<ReceiptPayload>();
    visit.visit::<MePayload>();
    visit.visit::<AboutPayload>();
    visit.visit::<PortalPayload>();
    visit.visit::<ReleasesPayload>();
    visit.visit::<AuthoredPayload>();
    visit.visit::<DevicePayload>();
    visit.visit::<MemoryPayload>();
    visit.visit::<InboxProposalsPayload>();
    visit.visit::<DrawnPayload>();
    visit.visit::<Frame>();
    visit.visit::<BlockRender>();
    visit.visit::<AttachmentRow>();
    visit.visit::<Uploaded>();
    visit.visit::<RevealPayload>();
    visit.visit::<ConversationsPayload>();
    visit.visit::<SearchPayload>();
    visit.visit::<StreamPayload>();
    visit.visit::<TopicsPayload>();
    visit.visit::<SayPayload>();
    visit.visit::<crate::usage::Report>();
    visit.visit::<Overview>();
    visit.visit::<ErrorPayload>();
    // A proposal's state register, as the module's readings hold it.
    visit.visit::<crate::secretary::phase::Phase>();
}

/// What a page sends: declared for the page to build and decoded by the box
/// alone, so the browser has no decoder for one.
fn bodies(visit: &mut impl TypeVisitor) {
    visit.visit::<Authoring>();
    visit.visit::<Titling>();
    visit.visit::<crate::secretary::routes::Erasing>();
    visit.visit::<Saying>();
    visit.visit::<Uploading>();
    visit.visit::<Tapping>();
    visit.visit::<Reasoning>();
    visit.visit::<KeyAsked>();
    visit.visit::<Pairing>();
    visit.visit::<Revealing>();
    visit.visit::<Registering>();
    visit.visit::<crate::clienterror::Sent>();
    visit.visit::<PostedObjects>();
}

/// The answers' TypeScript names, in order.
#[derive(Default)]
struct Names(Vec<String>);

impl TypeVisitor for Names {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        self.0.push(T::name());
    }
}

/// A walk over the roots' dependencies, each type's declaration after theirs.
#[derive(Default)]
struct Declarations {
    seen: std::collections::BTreeSet<std::any::TypeId>,
    out: Vec<String>,
}

impl TypeVisitor for Declarations {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if Dependency::from_ty::<T>().is_none() || !self.seen.insert(std::any::TypeId::of::<T>()) {
            return;
        }
        T::visit_dependencies(self);
        self.out.push(T::decl());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record's constructor name reads as the block it draws: `decision` alone is a
    /// decision, every other name a todo that keeps its name for the chip and the category.
    #[test]
    fn a_block_is_a_decision_or_a_named_todo() {
        assert_eq!(Block::of("decision"), Block::Decision);
        assert_eq!(
            Block::of("ekaTodo"),
            Block::Todo {
                name: "ekaTodo".to_owned()
            }
        );
        for name in crate::decoration::KINDS {
            assert_eq!(Block::of(name).name(), *name);
        }
        assert_eq!(
            serde_json::to_value(Block::of("todo")).expect("prints"),
            serde_json::json!({"kind": "todo", "name": "todo"})
        );
        assert_eq!(
            serde_json::to_value(Block::Decision).expect("prints"),
            serde_json::json!({"kind": "decision"})
        );
    }

    /// A fulfillment has three spellings, a number, `"absent"` and `null`,
    /// and each reads back as itself.
    #[test]
    fn a_fulfillment_is_a_number_absent_or_null_on_the_wire() {
        let cases: [(Result<Option<f64>, ()>, &str); 3] = [
            (Ok(Some(0.25)), "0.25"),
            (Ok(None), "\"absent\""),
            (Err(()), "null"),
        ];
        for (reading, spelled) in cases {
            let value = Fulfillment::of(reading);
            let text = serde_json::to_string(&value).expect("serialises");
            assert_eq!(text, spelled);
            let back: Option<Fulfillment> = serde_json::from_str(&text).expect("parses");
            assert_eq!(back, value);
        }
        assert!(serde_json::from_str::<Fulfillment>("\"none\"").is_err());
    }

    /// The wire's two files, written where `TS_RS_EXPORT_DIR` says: every
    /// answer has a decoder, folded from the text `wire.ts` holds.
    #[test]
    fn generates_the_frontend_s_wire_types() {
        let types = typescript();
        let decoders = decoders().expect("every answer has a decoder");
        if let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") {
            let dir = std::path::Path::new(&dir);
            std::fs::create_dir_all(dir).expect("the export directory is writable");
            std::fs::write(dir.join("wire.ts"), &types).expect("wire.ts is writable");
            std::fs::write(dir.join("payload.ts"), &decoders).expect("payload.ts is writable");
        }
    }

    /// Every body a page sends reads as its type and refuses a field it does not name, so a
    /// misspelt field is a 400 and never a default.
    #[test]
    fn every_body_refuses_a_field_it_does_not_name() {
        fn reads<T: serde::de::DeserializeOwned>(body: &serde_json::Value) {
            let mut extra = body.clone();
            let fields = match extra
                .as_object_mut()
                .expect("an object")
                .values_mut()
                .next()
            {
                // An enum's arm: the extra field goes inside it, where its fields are.
                Some(serde_json::Value::Object(arm))
                    if body.as_object().is_some_and(|o| o.len() == 1) =>
                {
                    arm
                }
                _ => extra.as_object_mut().expect("an object"),
            };
            fields.insert("colour".to_owned(), serde_json::json!("red"));
            serde_json::from_value::<T>(body.clone()).unwrap_or_else(|e| panic!("{body}: {e}"));
            let refused = serde_json::from_value::<T>(extra.clone())
                .err()
                .map(|e| e.to_string());
            assert!(
                refused
                    .as_deref()
                    .is_some_and(|said| said.contains("unknown field `colour`")),
                "{extra}: {refused:?}"
            );
        }
        let hash = "a".repeat(64);
        reads::<PostedObjects>(&serde_json::json!({"objects": ["Created(...)"]}));
        reads::<Authoring>(&serde_json::json!({"update": {"todo": "t", "body": "b", "note": "n"}}));
        reads::<Authoring>(&serde_json::json!({"confirm": {"todo": "t"}}));
        reads::<Authoring>(&serde_json::json!({"add": {
            "body": "b", "category": "home", "rationale": "r",
            "priority": {"how": "flat", "percent": 75},
        }}));
        reads::<Titling>(&serde_json::json!({"title": "t"}));
        reads::<Saying>(&serde_json::json!({"say": "s", "after": 2}));
        reads::<PickedFile>(&serde_json::json!({"id": hash}));
        reads::<Uploading>(&serde_json::json!({"file": "AAAA", "private": true}));
        reads::<Tapping>(&serde_json::json!({"note": "n", "always": true}));
        reads::<Endorsing>(&serde_json::json!({"key": "k", "signature": "s"}));
        reads::<Reasoning>(&serde_json::json!({"reason": "r"}));
        reads::<KeyAsked>(&serde_json::json!({"key": "k"}));
        reads::<Pairing>(&serde_json::json!({"code": "c"}));
        reads::<Revealing>(&serde_json::json!({"handle": "held:YQ.0"}));
        reads::<Registering>(&serde_json::json!({"platform": "android", "token": "t"}));
        reads::<crate::clienterror::Sent>(&serde_json::json!({"kind": "error", "message": "m"}));
    }

    /// The portal's answer is what `Pairing.java` parses, and a phone keeps an installed APK for
    /// months, so its JSON moves only together with that file.
    #[test]
    fn the_portal_answer_is_the_json_the_android_portal_reads() {
        use crate::connect::Channel;
        use crate::nonempty::NonEmpty;
        use crate::portal::{Portal, Sender};
        let sender = || Sender::try_from("100000000000".to_owned()).expect("a number");
        let fcm = NonEmpty::of(vec![Channel::Ntfy, Channel::Fcm]);
        let ntfy = NonEmpty::of(vec![Channel::Ntfy]);
        for (channels, sender, json) in [
            (
                fcm.as_ref(),
                Some(sender()),
                serde_json::json!({"push": {"kind": "fcm", "sender": "100000000000"}}),
            ),
            (
                fcm.as_ref(),
                None,
                serde_json::json!({"push": {"kind": "none"}}),
            ),
            (
                ntfy.as_ref(),
                Some(sender()),
                serde_json::json!({"push": {"kind": "none"}}),
            ),
            (
                None,
                Some(sender()),
                serde_json::json!({"push": {"kind": "none"}}),
            ),
        ] {
            let answer = PortalPayload::from(&Portal::of(channels, sender));
            assert_eq!(serde_json::to_value(&answer).expect("prints"), json);
        }
        for refused in ["", "12a", "+1000", &"1".repeat(21)] {
            assert!(Sender::try_from(refused.to_owned()).is_err(), "{refused:?}");
        }
    }

    /// An erasure names its turns, at least one, or says yes to the whole.
    #[test]
    fn an_erasure_is_some_turns_or_the_whole() {
        use crate::secretary::routes::Erasing;
        let read = |body: serde_json::Value| serde_json::from_value::<Erasing>(body);
        assert!(matches!(
            read(serde_json::json!({"turns": [0, 2]})),
            Ok(Erasing::Turns(_))
        ));
        assert_eq!(
            read(serde_json::json!({"whole": true})).expect("the whole"),
            Erasing::Whole(Yes)
        );
        for refused in [
            serde_json::json!({"turns": []}),
            serde_json::json!({"whole": false}),
            serde_json::json!({"turns": [0], "whole": true}),
            serde_json::json!({}),
        ] {
            assert!(read(refused.clone()).is_err(), "{refused}");
        }
    }

    /// The `/events` frame is the head set alone.
    #[test]
    fn a_tip_frame_is_the_heads_alone() {
        let frame = TipFrame {
            heads: vec!["ab12".into(), "cd34".into()],
        };
        assert_eq!(
            serde_json::to_string(&frame).expect("prints"),
            r#"{"heads":["ab12","cd34"]}"#
        );
    }

    /// Two types declaring one TypeScript name would be one type to the web.
    #[test]
    fn every_declared_name_is_one_type() {
        let generated = typescript();
        let mut names: Vec<&str> = generated
            .lines()
            .filter_map(|line| line.strip_prefix("export type "))
            .filter_map(|rest| rest.split([' ', '<']).next())
            .collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a TypeScript name is declared twice");
    }
}
