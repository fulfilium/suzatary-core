//! The web's payloads; their TypeScript and decoders are generated ([`shape`]). A field name is
//! the name on the wire, so renaming one is a contract change.

pub mod shape;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::{Dependency, TS, TypeVisitor};

use crate::reference::Reference;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Link {
    /// `literal` rehashes to it.
    pub hash: String,
    /// None at a genesis.
    pub parents: Vec<String>,
    pub literal: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub holds: Holds,
}

/// An envelope (a merge, a genesis, a signature) claims no todo, writer or instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "holds", rename_all = "lowercase")]
pub enum Holds {
    Envelope {
        kind: String,
    },
    Event {
        kind: String,
        todo: String,
        actor: String,
        /// ISO 8601.
        at: String,
        note: String,
        reverses: Reverses,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Reverses {
    Nothing,
    Undo {
        /// By hash.
        of: String,
    },
    /// Re-applies what the undo `of` undid.
    Redo {
        /// By hash.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuthoredSource {
    pub sender: String,
    pub subject: String,
    /// ISO 8601, date only.
    pub date: String,
    /// A prefix of the canonical hash; `/api/mail/message/<hash>` resolves it.
    pub hash: String,
    pub thread_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubTodo {
    /// Typst markup.
    pub body: String,
    pub done: bool,
}

/// Typst's HTML for the prose fields; `null` when absent, not rendered, or untrusted, and the page
/// shows the source beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Rich {
    pub body: Option<String>,
    pub detail: Option<String>,
    pub rationale: Option<String>,
    /// From string source.
    pub waiting_on: Option<String>,
    /// Each present field's HTML key; `null` for an absent or untrusted one.
    pub keys: RichKeys,
}

/// Each field's key in the rendered-HTML tabulation ([`crate::template::Template::key`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RichKeys {
    pub body: Option<String>,
    pub detail: Option<String>,
    pub rationale: Option<String>,
    /// Escaped.
    pub waiting_on: Option<String>,
}

/// A decision is a choice awaiting the owner's tap, never a todo on a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Block {
    Decision,
    /// `todo`, or one whose name files it (`ekaTodo`).
    Todo {
        name: String,
    },
}

impl Block {
    #[must_use]
    pub fn of(name: &str) -> Self {
        match name {
            "decision" => Self::Decision,
            other => Self::Todo {
                name: other.to_owned(),
            },
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Decision => "decision",
            Self::Todo { name } => name,
        }
    }
}

/// A todo's content as the confirmed fold sees it. `""` is absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Authored {
    pub rich: Rich,
    pub block: Block,
    /// ISO 8601 date only.
    pub created: String,
    /// Typst markup.
    pub body: String,
    /// [`crate::escape::markup_text`] of the markup, never rendered.
    pub text: String,
    /// Typst markup.
    pub detail: String,
    pub category: String,
    /// Typst string source.
    pub waiting_on: String,
    /// The `//` comment above the priority, line by line.
    pub rationale: Vec<String>,
    /// The authored FPL spec, printed by [`crate::term_json::to_json`].
    #[ts(type = "SpecNode | null")]
    pub spec: Option<serde_json::Value>,
    pub actor: String,
    /// ISO 8601.
    pub at: String,
    pub source: Option<AuthoredSource>,
    pub subtodos: Vec<SubTodo>,
}

/// The `From` matches `Outcome` with no wildcard, so a new binding fails the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum EntryState {
    Open,
    Completed,
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

/// What an untrusted actor's event would bind where the confirmed fold disagrees; `""` agrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ClaimedState {
    #[serde(rename = "")]
    Agrees,
    #[serde(rename = "completed")]
    Completed,
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

/// A number in [0, 1] (low is urgent), or `"absent"` for `∅`; `null` where it does not link.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Fulfillment {
    Number(f64),
    Absent(Absent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Absent {
    #[serde(rename = "absent")]
    Absent,
}

impl Fulfillment {
    /// `None` where it does not link.
    #[must_use]
    pub fn of<E>(value: Result<Option<f64>, E>) -> Option<Fulfillment> {
        value.ok().map(Fulfillment::of_value)
    }

    #[must_use]
    pub fn of_value(value: Option<f64>) -> Fulfillment {
        value.map_or(Fulfillment::Absent(Absent::Absent), Fulfillment::Number)
    }

    /// `None` for `∅`.
    #[must_use]
    pub fn number(self) -> Option<f64> {
        match self {
            Fulfillment::Number(value) => Some(value),
            Fulfillment::Absent(Absent::Absent) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Entry {
    pub todo: String,
    /// The binding a list files it under: open while any candidate is.
    pub state: EntryState,
    /// One unless the state is in conflict.
    pub states: Vec<EntryState>,
    /// `""` while open.
    pub at: String,
    /// Shown, never applied: the containment asymmetry made visible.
    pub claimed: ClaimedState,
    /// `null` where it does not link.
    pub value: Option<Fulfillment>,
    /// A pass restarts its price (`Tended`, "Did it") rather than completing it.
    pub recurring: bool,
    /// Provisional content, or a claim the trusted fold refuses.
    pub unconfirmed: bool,
    /// Registers with more than one live write, by hash.
    #[ts(type = "Record<string, string[]>")]
    pub conflicts: BTreeMap<String, Vec<String>>,
    /// `null` for a todo never authored; under a conflict, the first of `candidates`.
    pub authored: Option<Authored>,
    /// By object name, where the content is in conflict; else empty.
    pub candidates: Vec<Authored>,
    /// In causal order, by hash.
    pub stream: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Neighbour {
    pub todo: String,
    pub state: EntryState,
    pub value: Option<Fulfillment>,
    pub unconfirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FieldChange {
    pub field: String,
    /// `""` for the first record.
    pub before: String,
    pub after: String,
}

/// `GET /api/chain`: every object in the linearisation's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ChainPayload {
    /// Sorted.
    pub heads: Vec<String>,
    /// This deployment's trust policy, so an offline fold folds the same chain.
    pub untrusted: Vec<String>,
    /// The root a device key counts under.
    pub root: String,
    pub verify: Vec<String>,
    pub links: Vec<Link>,
}

/// `GET /api/fold`: every todo the chain has mentioned, at `t`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct FoldPayload {
    /// ISO 8601.
    pub at: String,
    /// Sorted: a tab keeps whichever of two folds read more.
    pub heads: Vec<String>,
    /// By id.
    pub entries: Vec<Entry>,
    /// What refers into one reads `∅` until it reads.
    pub failed: Vec<FailedChain>,
    /// When a whole percent or the belief next moves after `at`; `null` for never.
    pub next: Option<NextChange>,
}

/// An instant in the reading's zone; `exact` when something moves there, else a bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NextChange {
    /// ISO 8601.
    pub at: String,
    pub exact: bool,
}

impl NextChange {
    /// `None` for never.
    #[must_use]
    pub fn of(next: prodrome_core::observe::NextChange) -> Option<NextChange> {
        next.at.map(|at| NextChange {
            at: prodrome_core::fpl::iso(at),
            exact: next.exact,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct FailedChain {
    pub chain: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TodoLinks {
    pub out: Vec<Neighbour>,
    #[serde(rename = "in")]
    pub inward: Vec<Neighbour>,
}

/// `GET /api/todo/{id}`: one todo in full.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TodoPayload {
    pub todo: String,
    /// ISO 8601.
    pub at: String,
    pub entry: Entry,
    /// Why its number is what it is, part by part.
    pub why: crate::reading::Why,
    /// In causal order.
    pub stream: Vec<Link>,
    /// By that event's hash.
    #[ts(type = "Record<string, FieldChange[]>")]
    pub diffs: BTreeMap<String, Vec<FieldChange>>,
    pub links: TodoLinks,
    /// The routine "Build it" fires; `null` on a chain no `Completes:` closes.
    pub builder: Option<String>,
}

/// How `GET /api/tree` groups; ancestry nests under each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TreeBy {
    Category,
    /// Problem, Watch and Fine, by the root's value.
    Urgency,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum GroupBy {
    All,
    Category,
    Urgency,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Worth {
    Value(f64),
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
    Group {
        by: GroupBy,
        /// `""` for the root.
        label: String,
        worth: Worth,
        term: String,
        children: Vec<TreeNode>,
    },
    /// `Ref(todo)`, its subtodos as children.
    Todo {
        todo: String,
        /// Plain text.
        title: String,
        category: String,
        worth: Worth,
        term: String,
        children: Vec<TreeNode>,
    },
}

/// `GET /api/<chain>/tree`: the open todos as one tree of terms.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TreePayload {
    /// ISO 8601.
    pub at: String,
    pub by: TreeBy,
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
    /// Sorted.
    pub heads: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
pub enum StoreName {
    #[serde(rename = "inbox")]
    Inbox,
    #[serde(rename = "lines")]
    Lines,
    #[serde(rename = "topics")]
    Topics,
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
    pub store: StoreName,
    /// Sorted.
    pub heads: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BriefingRow {
    /// ISO 8601.
    pub date: String,
    pub title: String,
    pub words: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BriefRow {
    /// ISO 8601.
    pub date: String,
    pub conversation: String,
    pub turn: u32,
    pub title: String,
    pub words: u32,
    pub text: String,
}

/// `GET /api/briefings`: newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BriefingsPayload {
    /// Capped.
    pub briefings: Vec<BriefingRow>,
    /// Capped.
    pub briefs: Vec<BriefRow>,
}

/// `GET /api/briefings/{date}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BriefingPayload {
    /// ISO 8601.
    pub date: String,
    pub day: BriefingDay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BriefingDay {
    Brief {
        conversation: String,
        turn: u32,
        text: String,
        blocks: Vec<BlockRow>,
    },
    /// A file under `briefings/`, a trusted author's, so it renders unconditionally.
    Saved {
        /// Typst markup.
        source: String,
        rich: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(rename = "MailMessage")]
pub struct MailPage {
    #[serde(flatten)]
    #[ts(flatten)]
    pub message: crate::mail::MailMessage,
    pub todos: Vec<Entry>,
}

/// `GET /api/mail/inbox`: by date, with the roster for the filter chips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InboxPayload {
    pub accounts: Vec<String>,
    pub messages: Vec<crate::mail::MailRow>,
}

/// `GET /api/mail/watches`, and what a cancel answers: soonest to expire first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WatchesPayload {
    pub watches: Vec<MailWatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailWatch {
    pub id: String,
    pub on: WatchOn,
    /// Lower-cased.
    pub text: String,
    /// ISO 8601.
    pub set: String,
    /// ISO 8601.
    pub until: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum WatchOn {
    Sender,
    Subject,
}

/// `GET /api/mail/search`: a notmuch query, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailSearchPayload {
    pub q: String,
    pub accounts: Vec<String>,
    pub messages: Vec<crate::mail::MailRow>,
}

/// `GET /api/mail/thread/{id}`: oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ThreadPayload {
    pub thread: String,
    pub messages: Vec<MailPage>,
}

/// By its one key, so a body naming two or none is refused at the parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Authoring {
    Add(Box<NewTodo>),
    Update(Box<UpdateTodo>),
    Confirm(ConfirmTodo),
    Undo(String),
    Redo(String),
}

/// `POST /api/author` `{"add": …}`: the gate names the actor and the box stamps the instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NewTodo {
    /// An empty rationale gets the app's sentence.
    #[serde(flatten)]
    #[ts(flatten)]
    pub fields: crate::todo::TodoFields,
    /// Refused if the chain ever mentioned it; otherwise one from the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    /// Its spec gains `Ref(this)` in the same act.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<String>,
}

/// `POST /api/author` `{"update": …}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateTodo {
    pub todo: String,
    /// What is left out is kept.
    #[serde(flatten)]
    #[ts(flatten)]
    pub patch: crate::todo::TodoPatch,
    /// `""` for none.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub note: String,
}

/// `POST /api/author` `{"confirm": …}`: a provisional record a trusted actor takes as its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConfirmTodo {
    pub todo: String,
    /// `""` for none.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub note: String,
}

/// `POST /api/act/device`: a push token is a fact about a phone, not the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct DevicePayload {
    /// Always true; a failure is a refusal.
    pub ok: bool,
    pub devices: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StoredObject {
    /// sha256 of the literal, rehashed by the receiver.
    pub hash: String,
    pub literal: String,
}

/// The digest the box recorded its heads under, named back as `since`, and its object count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Frontier {
    /// 64 hex digits.
    pub digest: String,
    pub count: usize,
}

/// A `since` the box does not hold is answered as a replica holding nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum SinceRead {
    Known,
    Unknown,
}

/// `GET /api/objects?since=<frontier>`: what the caller lacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ObjectsPayload {
    /// Sorted.
    pub heads: Vec<String>,
    pub untrusted: Vec<String>,
    /// The root key a device key's signatures count under; `null` for a store the box alone writes.
    pub root: Option<String>,
    /// Sorted: a replica drops any other store it holds.
    pub geneses: Vec<String>,
    /// Bumped whenever `geneses` changes and by nothing else (`suzatary_core::epoch`).
    #[ts(type = "number")]
    pub epoch: u64,
    /// Parents first.
    pub objects: Vec<StoredObject>,
    pub since: SinceRead,
    /// To name as the next `since`.
    pub frontier: Frontier,
}

/// `GET /api/stores/<store>/objects?since=<heads>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StorePayload {
    #[serde(flatten)]
    #[ts(flatten)]
    pub objects: ObjectsPayload,
    /// Every sent slot's key, hex; never one an erasure names, so no erased word opens.
    #[ts(type = "Record<string, string>")]
    pub keys: BTreeMap<String, String>,
    /// None when the answer holds all it lacked; else the replica asks again from its heads.
    pub rest: usize,
    /// The bytes of the literals the replica lacked when it asked, these and the rest.
    pub size: usize,
    /// So `size` less this is what is left.
    pub weight: usize,
    /// Sorted: keys a replica forgets the blocks of before the erasure arrives.
    pub erased: Vec<String>,
}

/// `GET /api/stores/<store>/content?hashes=<hashes>`: literals, sent `Cache-Control: immutable`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ContentPayload {
    /// One per hash asked, in the order asked.
    pub objects: Vec<StoredObject>,
}

/// What a fold reads of an object without its literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SkeletonNode {
    pub hash: String,
    /// Its prodrome's genesis included, sorted.
    pub parents: Vec<String>,
    /// The bytes of its literal, so a tab can ask for content in answers of a bounded size.
    pub size: usize,
    pub header: NodeHeader,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "node", rename_all = "lowercase")]
pub enum NodeHeader {
    Structure {
        kind: String,
    },
    Line {
        conversation: String,
        /// By its writer's clock.
        at: String,
        kind: String,
        /// In slot order.
        keys: Vec<String>,
        step: crate::secretary::step::Step,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "are", rename_all = "lowercase")]
pub enum Heads {
    /// The heads the asker's `since` names, so they are not sent again.
    Held,
    These {
        /// Sorted.
        heads: Vec<String>,
    },
}

/// `GET /api/stores/lines/skeleton?since=<heads>`: the lines without literals; no erased key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SkeletonPayload {
    pub heads: Heads,
    /// Sorted.
    pub geneses: Vec<String>,
    #[ts(type = "number")]
    pub epoch: u64,
    /// Parents first.
    pub nodes: Vec<SkeletonNode>,
    pub since: SinceRead,
    /// To name as the next `since`.
    pub frontier: Frontier,
    #[ts(type = "Record<string, string>")]
    pub keys: BTreeMap<String, String>,
    pub rest: usize,
    /// The bytes of the nodes lacked, by a piece's weight.
    pub size: usize,
    /// By the same weight, so `size` less this is what is left.
    pub weight: usize,
}

/// `POST /api/objects`: literals only; a name is what the bytes are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PostedObjects {
    /// Canonical prints, in any order.
    pub objects: Vec<String>,
    /// As a pull names it: absent or empty for none.
    #[serde(default)]
    pub since: String,
}

/// `POST /api/secretary/conversations` and `…/title`: `""` leaves it to the title job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Titling {
    pub title: String,
}

/// `POST …/say` and `…/edit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Saying {
    pub say: String,
    /// In its one text form (`todo:life/x`).
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub references: Vec<crate::reference::Reference>,
    #[serde(default)]
    #[ts(as = "Option<Vec<PickedFile>>", optional)]
    pub attachments: Vec<PickedFile>,
    /// How many turns the asker saw; absent, a question is always a new line.
    #[serde(default)]
    #[ts(optional)]
    pub after: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PickedFile {
    /// 64 hex.
    #[ts(as = "String")]
    pub id: crate::attachment::AttachmentId,
    /// `""` for a photo.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub name: String,
}

/// `POST /api/secretary/attachments`: base64 in JSON, under the same content-type rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Uploading {
    /// Base64.
    pub file: String,
    /// `""` for a photo.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub name: String,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub private: bool,
}

/// `POST …/proposals/<n>/tap`: an index in its path, never a body that could change the act.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Tapping {
    /// `""` for none.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub note: String,
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub value: Option<crate::capability::Secret>,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub always: bool,
    #[serde(default)]
    #[ts(optional)]
    pub endorsement: Option<Endorsing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Endorsing {
    /// 64 hex.
    pub key: String,
    /// 128 hex.
    pub signature: String,
}

/// `POST …/dismiss` and `…/drop`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Reasoning {
    pub reason: String,
}

/// `POST /api/act/key`: a device asking to be trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KeyAsked {
    /// 64 hex.
    pub key: String,
}

/// `POST /api/act/pair`: the code `suzatary device pair` printed at the box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Pairing {
    pub code: String,
}

/// `POST /api/held/reveal`: the owner's tap on a held value's placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Revealing {
    /// As the placeholder prints it (`held:…`).
    pub handle: String,
    /// Must have read a file the handle names; `null` on a mail page.
    pub conversation: Option<String>,
}

/// The box keeps [`RevealPayload::NotKept`] for a replay of the request, never the value.
#[derive(Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum RevealPayload {
    Shown {
        value: String,
    },
    File {
        name: String,
        /// `type/subtype`, read from its bytes.
        media: String,
        /// Base64.
        bytes: String,
    },
    Gone,
    /// A replay: the value was answered once and not kept; tap again.
    NotKept,
}

impl RevealPayload {
    /// # Errors
    /// A file over [`crate::held_value::MAX_OPENED`], which no answer carries.
    pub fn of(read: crate::held_value::Revealed) -> Result<Self, crate::refusal::Refusal> {
        use base64::Engine as _;
        match read {
            crate::held_value::Revealed::Value(shown) => Ok(Self::Shown {
                value: shown.into_string(),
            }),
            crate::held_value::Revealed::File(opened) => Ok(Self::File {
                name: opened.name,
                media: opened.media,
                bytes: base64::engine::general_purpose::STANDARD.encode(opened.bytes),
            }),
            crate::held_value::Revealed::TooLarge(size) => Err(crate::refusal::Refusal::new(
                crate::refusal::Status::UnprocessableEntity,
                format!(
                    "the file is {size} bytes; a tap opens at most {} here, so open it where \
                     it is kept",
                    crate::held_value::MAX_OPENED
                ),
            )),
            crate::held_value::Revealed::Gone => Ok(Self::Gone),
        }
    }
}

impl std::fmt::Debug for RevealPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Shown { .. } => "Shown(a held value)",
            Self::File { .. } => "File(a held file)",
            Self::Gone => "Gone",
            Self::NotKept => "NotKept",
        })
    }
}

/// `POST /api/act/device`, signed by the page, since the Android portal holds no key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Registering {
    pub platform: crate::devices::Platform,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RefusedObject {
    /// `""` when the bytes were not an object.
    pub hash: String,
    pub reason: String,
    pub kind: RefusalKind,
}

/// A sum, because the tab branches on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RefusalKind {
    /// Held and named, and everything resting on it with it.
    Final,
    /// Taken once its device is trusted and it is signed.
    Trust,
}

/// `POST /api/objects`. The lists are always present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReceiptPayload {
    /// Sorted.
    pub heads: Vec<String>,
    /// Parents first.
    pub adopted: Vec<String>,
    /// What makes a re-post free.
    pub already: Vec<String>,
    /// In arrival order.
    pub refused: Vec<RefusedObject>,
    pub since: SinceRead,
    /// What it held at `since` and everything adopted, with all they rest on.
    pub frontier: Frontier,
}

/// `GET /api/portal`: what the Android portal reads from a box before it keeps it as its box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct PortalPayload {
    pub push: PortalPush,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PortalPush {
    None,
    Fcm { sender: String },
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

/// `GET /api/about`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AboutPayload {
    /// `dev` when unstamped.
    pub version: String,
    /// Short git revision, or `dev` outside the flake.
    pub build: String,
    /// From `app/release.toml`, first named first.
    pub completes: Vec<Shipped>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Shipped {
    pub todo: String,
    /// `null` where the chain is not served or does not hold the todo.
    pub held: Option<HeldTodo>,
}

/// `GET /api/releases`: the releases the project chain reads, each with its items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReleasesPayload {
    pub chain: String,
    /// The `major.minor` it reads now, which the next train starts from.
    pub reads: String,
    /// In the order they ship; none where the chain is not served.
    pub releases: Vec<ReleaseRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReleaseRow {
    pub todo: String,
    /// `null` while the plan declares a todo the chain does not hold yet.
    pub held: Option<HeldTodo>,
    pub version: String,
    pub lot: crate::release::Lot,
    pub place: crate::release::Place,
    /// Every todo its spec has named, in id order.
    pub items: Vec<ReleaseItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReleaseItem {
    pub todo: String,
    /// `null` where the spec names a todo the chain does not hold.
    pub held: Option<HeldTodo>,
    pub lot: crate::release::Lot,
    pub named: crate::release::Named,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct HeldTodo {
    pub row: Neighbour,
    /// Plain text.
    pub title: String,
}

/// `GET /api/me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MePayload {
    /// The Tailscale login, `""` for the local shell.
    pub login: String,
    /// `null` for a viewer (or a mapping this chain's roster does not know).
    pub actor: Option<String>,
    /// In the instance file's order.
    pub chains: Vec<String>,
    /// In the deployment's order, so no page spells the vocabulary itself.
    pub categories: Vec<String>,
    /// The owner's zone: where a day is read and a time printed.
    pub zone: String,
}

/// `POST /api/author`: the authoring without the append; the replica seals the event itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuthoredPayload {
    pub todo: String,
    pub event: String,
    pub revision: Option<String>,
}

/// Every refusal: the server's own sentence, shown verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ErrorPayload {
    pub error: String,
}

/// In prose for somebody deciding, where [`FieldChange`] carries the canonical print as evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposedChange {
    pub field: String,
    /// `""` when absent.
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposedObject {
    /// From 1.
    pub position: usize,
    #[serde(flatten)]
    #[ts(flatten)]
    pub act: ObjectAct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "act", rename_all = "lowercase")]
pub enum ObjectAct {
    Added {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        changes: Vec<ProposedChange>,
    },
    Edited {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        changes: Vec<ProposedChange>,
    },
    Repriced {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        changes: Vec<ProposedChange>,
    },
    Created {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        note: String,
    },
    Completed {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        note: String,
    },
    Cancelled {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        note: String,
    },
    Reopened {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        note: String,
    },
    Tended {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        note: String,
    },
    Respecified {
        #[serde(flatten)]
        #[ts(flatten)]
        todo: ProposedTodo,
        note: String,
    },
}

impl ObjectAct {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposedTodo {
    pub todo: String,
    /// In prose, as of this object; `""` while nothing has authored one.
    pub body: String,
    pub at: String,
    pub actor: String,
    /// Its todo has no content on the chain: it waits on another card.
    pub pending_todo: bool,
}

/// The route's match over `Turn` has no wildcard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "who", rename_all = "lowercase")]
pub enum Speaker {
    Person {
        #[ts(type = "Array<string>")]
        references: Vec<Reference>,
        attachments: Vec<AttachmentRow>,
        queued: bool,
    },
    Model {
        /// Split at top-level blank lines.
        blocks: Vec<BlockRow>,
    },
    Verb {
        verb: String,
        outcome: VerbOutcome,
    },
    Act {
        #[ts(type = "Array<string>")]
        references: Vec<Reference>,
    },
    /// A note of the box's own state, or another's words it relays; never the owner's.
    Box {
        #[ts(type = "Array<string>")]
        references: Vec<Reference>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Attempt {
    Current,
    Earlier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AttachmentRow {
    pub id: String,
    /// `application/pdf` or an image type.
    pub media_type: String,
    /// `""` for a photo, which the phone redraws.
    pub name: String,
    /// Where in the owner's Drive; `""` for a file on the box.
    pub private: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Uploaded {
    Kept {
        file: AttachmentRow,
    },
    /// Names an identity number: the owner's Drive or nowhere, on his say.
    Sensitive {
        why: String,
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

/// Tagged by speaker. `text` is the least trusted string served and is drawn as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TurnRow {
    /// Absolute; an `?about=` window has gaps.
    pub index: usize,
    pub text: String,
    pub at: String,
    /// `index` still names an earlier attempt's turn.
    pub attempt: Attempt,
    /// When set, `text` is the box's sentence for it, never anybody's words.
    pub gone: Option<TurnGone>,
    #[serde(flatten)]
    #[ts(flatten)]
    pub who: Speaker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "why", rename_all = "lowercase")]
pub enum TurnGone {
    Asked {
        by: String,
    },
    Forgot {
        fact: crate::secretary::memory::FactId,
    },
    /// Their key is not here: lost in a restore, or never given to this replica.
    Missing,
    /// Locked under older redaction rules, they do not pass the current ones.
    Withheld,
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
            Gone::Withheld => Self::Withheld,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BlockRow {
    /// Its typst with the blank line that ends it.
    pub source: String,
    pub plain: String,
    pub kind: crate::secretary::answer::Kind,
    /// Computed on read; `null` when it names `html`, fails, or trips the wire.
    pub html: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "is", rename_all = "lowercase")]
pub enum VerbOutcome {
    Answered,
    Refused,
    Remembered { receipt: Receipt },
}

/// Its words are read from the memory by number, so a forgotten fact's receipt has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Receipt {
    Standing {
        fact: crate::secretary::memory::FactId,
        /// Model text, drawn as text.
        text: String,
    },
    Forgotten {
        fact: crate::secretary::memory::FactId,
    },
    /// By a consolidation: its words are gone, and `into` says what they said.
    Merged {
        fact: crate::secretary::memory::FactId,
        into: crate::secretary::memory::FactId,
    },
    /// The memory would not read, or holds no words for it that are not forgotten.
    Unread {
        fact: crate::secretary::memory::FactId,
    },
}

/// `GET /api/secretary/memory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MemoryPayload {
    /// Oldest first.
    pub facts: Vec<FactRow>,
    pub cap: u32,
    /// Oldest first: a receipt naming one says so.
    pub forgotten: Vec<crate::secretary::memory::FactId>,
    /// By the consolidation: a receipt naming one says so.
    pub merged: Vec<MergedFact>,
    /// By the name a card gives them: a card whose name is not here has had its words deleted.
    pub cards: Vec<CardWords>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardWords {
    /// 64 hex.
    pub text: String,
    /// Model text, drawn as text.
    pub fact: String,
}

/// `GET /api/secretary/record`: the summaries a session opening is told, each retractable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecordPayload {
    /// Oldest first.
    pub summaries: Vec<RecordSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecordSummary {
    /// Content name, 64 hex, which the retract route names.
    pub node: String,
    /// `first+lines`.
    pub handle: String,
    pub lines: u32,
    /// Model text, drawn as text.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MergedFact {
    pub fact: crate::secretary::memory::FactId,
    pub into: crate::secretary::memory::FactId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FactRow {
    pub id: crate::secretary::memory::FactId,
    pub at: String,
    /// Model text, drawn as text.
    pub fact: String,
    pub kind: crate::secretary::memory::Kind,
    pub said: FactSaid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "how", rename_all = "lowercase")]
pub enum FactSaid {
    Said {
        on: SaidOn,
    },
    Merged {
        /// By number.
        from: Vec<crate::secretary::memory::FactId>,
        /// At least one.
        origins: Vec<SaidOn>,
        round: SaidOn,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SaidOn {
    Turn { conversation: String, turn: u32 },
    Tapped { conversation: String, proposal: u32 },
}

/// `outcome` beside it stays free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ProposalState {
    Standing,
    Sent,
    /// A tap failed and nothing happened: Try again or Dismiss.
    Failed,
    /// A tap was refused for a cause a tap again meets, and nothing happened: Dismiss.
    Refused,
    /// A tap did part of the act and stopped: Try again finishes it, or Dismiss.
    Unfinished,
    /// A tap failed and it may have happened: never performed again until checked.
    Unknown,
    /// A send that may have gone, which the owner has checked.
    Checked,
    Dismissed,
    /// A later draft replaced it unseen, so it is not the owner's no.
    Superseded,
    /// The box or the secretary took it back; `outcome` says why. Not the owner's no.
    Withdrawn,
    /// Taken back because it was done another way; `outcome` says what the box saw.
    DoneElsewhere,
    /// A changeset's acts are on the chain; nothing left the box.
    Applied,
    /// An act's device-code flow is running; a second tap is refused.
    Started,
    Acted,
    /// Its responses read as no state (a check of nothing): shown with why, never acted on.
    Unreadable,
}

/// What the box keeps as a connection's cache, which removing it may delete too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Cached {
    Nothing,
    Mail,
}

/// A tagged union; each act's `sentence` is `Act::sentence`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ProposedAct {
    Email {
        account: String,
        to: Vec<String>,
        cc: Vec<String>,
        /// Verbatim.
        subject: String,
        /// Verbatim.
        body: String,
        attachments: Vec<CardFile>,
        held: HeldIn,
    },
    Forward {
        account: String,
        to: Vec<String>,
        cc: Vec<String>,
        /// Verbatim.
        subject: String,
        /// The note above the original, verbatim.
        note: String,
        /// The original's attachments inline, or its `.eml`.
        attachments: Vec<CardFile>,
        original: String,
        style: crate::secretary::store::Style,
        /// As "SSN ending 3456".
        held: Vec<String>,
    },
    Changes {
        chain: String,
        changes: Vec<ProposedObject>,
        /// Why an act would no longer build, `""` while it would.
        problem: String,
    },
    Auth {
        /// Verbatim.
        account: String,
        sentence: String,
    },
    /// A Microsoft account's consent to send: the device-code sign-in for SMTP alone.
    #[serde(rename = "auth_send")]
    AuthSend {
        /// Verbatim.
        account: String,
        sentence: String,
    },
    /// No value field: the card's own password field sends it in the tap's body.
    Secret {
        /// What the owner calls it; its stored name stays on the box.
        label: String,
        /// For a secret a template knows.
        found: Option<String>,
        /// The page that issues it, for a secret a template knows.
        link: Option<String>,
    },
    /// `target` is exactly what would leave the box.
    Web {
        request: crate::web::WebKind,
        target: String,
        host: String,
        /// The drafted "Always allow" rule, `""` for none.
        always: String,
        sentence: String,
    },
    /// The tap pushes exactly `patch`.
    Patch {
        /// `owner/name`.
        repo: String,
        visibility: crate::repos::Visibility,
        base: String,
        message: String,
        files: Vec<String>,
        /// Unified diff, verbatim.
        patch: String,
        sentence: String,
    },
    Drive {
        path: String,
        name: String,
        /// What "Always allow" would name.
        folder: String,
        /// In words, `""` when unknown.
        size: String,
        sentence: String,
    },
    /// The tap fires the routine with exactly `task`.
    #[serde(rename = "coding_session")]
    CodingSession {
        task: String,
        sentence: String,
    },
    /// The tap is endorsed by a device already trusted.
    Device {
        actor: String,
        /// 64 hex.
        key: String,
        sentence: String,
    },
    #[serde(rename = "sign_in")]
    SignIn {
        sentence: String,
    },
    #[serde(rename = "unlock_store")]
    UnlockStore {
        sentence: String,
    },
    Erase {
        sentence: String,
    },
    Retract {
        sentence: String,
    },
    Afresh {
        sentence: String,
    },
    /// Leave a sender out of triage: `sender` was read off the queued `message`, a hash.
    Noise {
        sender: String,
        message: String,
        sentence: String,
    },
    /// The code an open sign-in waits for; the password field sends it in the tap's body.
    #[serde(rename = "sign_in_code")]
    SignInCode {
        url: String,
        sentence: String,
    },
    /// The box's Google Cloud client: the password field takes its downloaded file.
    #[serde(rename = "google_client")]
    GoogleClient {
        /// The client must list it as an authorized redirect URI.
        redirect: String,
        steps: Vec<String>,
        sentence: String,
    },
    #[serde(rename = "google_sign_in")]
    GoogleSignIn {
        account: String,
        parts: Vec<String>,
        /// While the sign-in runs, the box's path on to Google's consent page; `null` otherwise.
        consent: Option<String>,
        sentence: String,
    },
    #[serde(rename = "calendar_event")]
    CalendarEvent {
        account: String,
        event: String,
        sentence: String,
    },
    Connect {
        /// What the owner calls it: `your Work calendar`, an address, a folder.
        connection: String,
        writes: String,
        steps: Vec<String>,
        /// The origins its plugin may reach from the start; empty for a connection of no plugin.
        reach: Vec<String>,
        sentence: String,
    },
    Disconnect {
        connection: String,
        writes: String,
        /// The secrets its steps stored, in the owner's words: the removal deletes them.
        forgets: Vec<String>,
        /// Its wider yes also deletes it.
        cached: Cached,
        sentence: String,
    },
    /// Only the origin, never the link it was read from.
    Grant {
        plugin: String,
        origin: String,
        sentence: String,
    },
    /// A fact to remember from a round that read something not the owner's own.
    Remember {
        words: RememberWords,
        about: crate::secretary::memory::Kind,
        /// The imperative, or what became of the words.
        sentence: String,
    },
    /// A fact to write in place of the standing facts it `replaces`, from such a round.
    Consolidate {
        words: RememberWords,
        about: crate::secretary::memory::Kind,
        /// At least one.
        replaces: Vec<ReplacedFact>,
        sentence: String,
    },
    /// The tap records exactly these fields as the owner's.
    Decision {
        id: String,
        title: String,
        /// Typst markup as written.
        body: String,
        rationale: String,
        supersedes: Vec<String>,
        sentence: String,
    },
}

/// The card names them and the memory's text store keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum RememberWords {
    Kept {
        fact: String,
    },
    /// The card was dismissed, or the fact it became forgotten.
    Gone {},
    /// The reading was given no memory, so it cannot say.
    Unread {},
}

/// A fact a consolidation card replaces, with its words while it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReplacedFact {
    pub id: crate::secretary::memory::FactId,
    /// `Gone` once the fact no longer stands: forgotten, or replaced by this card's tap.
    pub words: RememberWords,
}

/// The page tags a card by it and folds the setup cards into one Setup row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Asker {
    Secretary {},
    /// Reads mail anyone can send.
    Triage {},
    Plugin {
        name: String,
    },
    /// The box, with no model asked: a declared secret nothing stores.
    Setup {},
    Box {},
}

/// Decided by the box, so the page knows no kinds. The card is the confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Card {
    pub title: String,
    pub by: Asker,
    /// `""` when the fields say it.
    pub sentence: String,
    pub fields: Vec<CardField>,
    /// A verbatim block under the fields, `""` for none.
    pub body: String,
    pub rows: Vec<ProposedObject>,
    pub files: Vec<CardFile>,
    /// Why it cannot be performed as it stands; a card with one offers no yes.
    pub problem: String,
    /// The first is the main one.
    pub answers: Vec<CardAnswer>,
    pub uncertain: String,
    /// The answer that says the owner did that check.
    pub check: String,
    /// What a tap that did nothing is called before its reason: "Not sent" for mail.
    pub failed: String,
    pub readonly: String,
    /// `null` for none; never a model's words drawn as a button.
    pub link: Option<CardLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CardLink {
    Page {
        label: String,
        /// An https page, or a path on the box itself.
        href: String,
    },
    /// Begins the setup conversation on a template, through `POST /api/secretary/setup/<template>`.
    Setup { label: String, template: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum HeldIn {
    /// In the order the body names them.
    Read { values: Vec<HeldFrom> },
    /// A handle that does not decode; the tap refuses the send.
    Unreadable { why: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HeldFrom {
    /// `SSN ending 3456`.
    pub named: String,
    /// In the box's words: its sender, subject and date, or that it is gone.
    pub from: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardFile {
    pub name: String,
    /// `type/subtype`.
    pub media: String,
    pub size: String,
    /// Hex: what the tap sends is exactly these bytes.
    pub sha256: String,
    pub from: FileFrom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FileFrom {
    Mail {
        message: String,
    },
    Drive {
        path: String,
    },
    Upload {
        id: String,
    },
    /// A file the owner allowed once, the copy read into this conversation.
    Kept {},
    Repo {
        /// `owner/name`.
        repo: String,
        path: String,
        at: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardField {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardAnswer {
    /// `send`, `confirm`, `run`, …; the button's class is `act-<verb>`.
    pub verb: String,
    pub label: String,
    pub done: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub asks: Asks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "asks", rename_all = "lowercase")]
pub enum Asks {
    Now,
    /// The tap carries the owner's note.
    Confirm {
        question: String,
        yes: String,
    },
    /// The tap carries `always`, the card's wider yes.
    Always {
        question: String,
        yes: String,
    },
    /// The tap carries this device's signature of the card, a yes only a trusted device can give.
    Endorse {
        question: String,
        yes: String,
    },
    /// Sent only in the tap's body, with no confirm step.
    Value {
        input: String,
    },
}

/// In the order their indices were asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DrawnPayload {
    /// One per index asked.
    pub proposals: Vec<ProposalRow>,
}

/// Every field whole: the tap names an index, so what is shown is exactly what is sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposalRow {
    /// What a tap names.
    pub index: usize,
    pub act: ProposedAct,
    pub card: Card,
    pub why: String,
    /// Never empty.
    #[ts(type = "Array<string>")]
    pub sources: Vec<Reference>,
    pub at: String,
    pub state: ProposalState,
    /// The transport's words, the recipients, or the owner's reason; `""` while standing.
    pub outcome: String,
    pub tries: u32,
    /// Turns standing before it; drawn after turn `turn - 1`, never renumbered.
    pub turn: usize,
    pub coding: Option<CodingSession>,
    /// While its act runs, what the box said it waits for; `None` otherwise.
    pub announced: Option<String>,
    /// In the same conversation; `None` for a first asking.
    pub revises: Option<Revises>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Revises {
    pub card: usize,
    /// Changeset rows the replaced card already asked for; `0` unless both are changesets.
    pub carried: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct InboxProposal {
    pub id: String,
    #[ts(type = "string")]
    pub asked: crate::reference::ProposalRef,
    #[ts(type = "string | null")]
    pub serves: Option<crate::reference::TodoRef>,
    pub headline: String,
    pub by: Asker,
    pub why: String,
    pub state: InboxState,
    pub value: Option<Fulfillment>,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum InboxState {
    Open {},
    Completed {
        at: String,
    },
    Cancelled {
        at: String,
    },
    /// Never waiting, since nothing acts on it.
    Unreadable {
        reason: String,
    },
}

/// `GET /api/waiting`: read on the box so a tab need not hold the conversations to show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WaitingPayload {
    /// Open proposals, open decisions (one per batch they name), and lines no record places.
    pub count: usize,
}

/// `GET /api/inbox`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct InboxProposalsPayload {
    pub at: String,
    /// Oldest first.
    pub proposals: Vec<InboxProposal>,
    pub failed: Vec<FailedChain>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum CodingSession {
    Fired {
        id: String,
        url: String,
        at: String,
    },
    Reported {
        id: String,
        branch: String,
        /// `docs/reports/<slug>.md`.
        path: String,
        /// Up to its cut.
        report: String,
        stat: Vec<CodingFile>,
    },
    Merged {
        id: String,
        sha: String,
    },
    Dropped {
        id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CodingFile {
    pub path: String,
    /// `null` for a binary file.
    pub added: Option<u32>,
    /// `null` for a binary file.
    pub removed: Option<u32>,
}

/// One SSE frame of `GET …/stream`; `Turn` says the transcript grew, refetch it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Frame {
    State {
        state: crate::secretary::store::ConversationState,
    },
    /// Verbatim, drawn as text.
    Delta {
        text: String,
    },
    /// Cards come only from the refetch.
    Turn,
    Working {
        verb: String,
    },
    /// Rendered by `sha`, never by text, so the page cannot make the box render other words.
    Block {
        /// From 0.
        n: usize,
        /// sha256 of `source`, hex.
        sha: String,
        source: String,
    },
}

/// `GET …/blocks/<sha>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BlockRender {
    /// Through the render tripwire.
    pub html: String,
    /// [`crate::template::Template::key`] of the answer template at the block.
    pub key: String,
}

/// `GET /api/secretary/conversations/<id>`, windowed and filtered; every index is absolute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConversationPayload {
    pub id: String,
    pub title: String,
    pub model: String,
    pub opened: String,
    pub turns: Vec<TurnRow>,
    /// Every proposal whose round was kept, never windowed.
    pub proposals: Vec<ProposalRow>,
    /// Never windowed.
    pub unplaced: Vec<UnplacedProposal>,
    /// The page polls while this is not `idle`.
    pub state: crate::secretary::store::ConversationState,
    /// The earliest index carried, `total` when empty.
    pub first: usize,
    pub total: usize,
    /// As `Stored::rewind` would take it.
    pub again: Option<usize>,
}

/// Drawn with why, and nothing acts on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UnplacedProposal {
    pub index: usize,
    pub why: crate::secretary::store::Unheld,
}

/// One row of `GET /api/secretary/conversations`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConversationRow {
    pub id: String,
    pub title: String,
    pub kind: crate::secretary::store::Kind,
    pub opened: String,
    pub turns: u32,
    pub standing: usize,
    pub unplaced: Vec<UnplacedProposal>,
    pub last: String,
    pub last_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SearchPayload {
    pub query: String,
    /// Newest first.
    pub hits: Vec<crate::secretary::search::Hit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StreamPayload {
    /// The newest a page asked for, oldest first.
    pub rounds: Vec<StreamRound>,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StreamRound {
    /// `round:<conversation>/<turn>`, the target its topics are on.
    pub target: String,
    pub conversation: String,
    pub title: String,
    pub turn: usize,
    pub opening: TurnRow,
    pub asked: RoundAsked,
    /// Its last model turn with words; absent while it is going or when it said nothing.
    pub answer: Option<TurnRow>,
    /// Each in its current spelling, sorted.
    pub topics: Vec<RoundTopic>,
    pub room: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RoundTopic {
    pub topic: String,
    pub by: crate::secretary::topics::provenance::Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "by", rename_all = "lowercase")]
pub enum RoundAsked {
    Person {},
    /// One of the box's own rounds: `title` in its place, the question behind a disclosure.
    Box {
        title: String,
    },
}

/// The most rounds first (`topics::reading::in_use`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TopicsPayload {
    pub topics: Vec<TopicInUse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TopicInUse {
    pub topic: String,
    pub rounds: usize,
    /// Names concurrent renames disagree on, sorted; empty when none do.
    pub disputed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConversationsPayload {
    /// Newest first.
    pub conversations: Vec<ConversationRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SayPayload {
    pub conversation: ConversationPayload,
}

/// `GET /api/overview`: never a secret's name or value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Overview {
    /// In `Group::ALL`'s order; one nothing is connected to holds no line.
    pub groups: Vec<OverviewGroup>,
    pub ask: String,
    /// From the catalog, in the order Settings offers it.
    pub offers: Vec<Offer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Offer {
    /// What `POST /api/secretary/setup/<template>` takes.
    pub template: String,
    pub group: crate::connect::Group,
    /// As a sentence ends with it: `a Google account (its mail and calendars)`.
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OverviewGroup {
    pub group: crate::connect::Group,
    pub lines: Vec<OverviewLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OverviewLine {
    /// Its identity within its group, never a secret's name.
    pub key: String,
    pub label: String,
    pub detail: Option<String>,
    pub standing: crate::connect::Standing,
    /// Each a `proposal:<conversation>/<n>`; every need's on one line only.
    pub cards: Vec<String>,
    /// Each stored secret it reads that a Secret card can replace.
    pub replaces: Vec<Replace>,
}

/// A stored secret a line offers to replace, by `POST /api/secretary/replace/<handle>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Replace {
    /// What the owner calls it.
    pub label: String,
    /// How the route names it: never the secret's name.
    pub handle: String,
}

/// `GET /api/calendar/{date}`: a week of the owner's calendars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CalendarPayload {
    /// ISO 8601.
    pub from: String,
    pub days: u32,
    pub text: String,
    /// `null` where nothing rendered them.
    pub rich: Option<String>,
}

/// `Todos.verify_history` in the browser's module: SPEC §3's verify of a tab's objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct VerifiedPayload {
    pub tips: Vec<String>,
    /// Walk order, heads first, then any object the walk never reached.
    pub objects: Vec<ObjectVerdict>,
    pub walked: u32,
    pub verified: u32,
    pub genesis: Vec<String>,
    pub linearisation: Vec<String>,
    pub found: Found,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ObjectVerdict {
    /// The name it claims, which its children's parents point at.
    pub hash: String,
    pub computed: String,
    #[serde(rename = "hashOk")]
    pub hash_ok: bool,
    pub walk: Walk,
    pub condition: Condition,
    #[serde(flatten)]
    #[ts(flatten)]
    pub holds: Verified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "holds", rename_all = "lowercase")]
pub enum Verified {
    Envelope {
        kind: String,
    },
    Event {
        kind: String,
        todo: String,
        actor: String,
        /// ISO 8601.
        at: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Condition {
    Clean,
    Flawed { problem: String },
}

impl Condition {
    /// The Prodrome's print, which says clean with an empty sentence.
    #[must_use]
    pub fn of(problem: String) -> Self {
        if problem.is_empty() {
            Self::Clean
        } else {
            Self::Flawed { problem }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Walk {
    /// At its distance from the nearest head (0 is a head).
    Reached {
        depth: u32,
    },
    Unreached,
}

/// Every object clean and reached, or every problem in the order found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Found {
    Reached,
    Unreached {
        #[ts(type = "Array<string>")]
        problems: crate::nonempty::NonEmpty<String>,
    },
}

impl Found {
    #[must_use]
    pub fn of(problems: Vec<String>) -> Self {
        crate::nonempty::NonEmpty::of(problems)
            .map_or(Self::Reached, |problems| Self::Unreached { problems })
    }
}

/// `Todos.provenance` in the browser's module: who wrote one object carrying an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ProvenanceRow {
    pub hash: String,
    pub proof: Proof,
    pub said: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Proof {
    Proven {
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
    pub hash: String,
    pub text: String,
}

/// [`crate::term_json::to_json`]'s shape, open: nothing derives `TS` for a term.
pub const SPEC_NODE: &str = "type SpecNode = { kind: string } & Record<string, unknown>;";

const HEADER: &str = "\
// GENERATED FROM app/core/src/wire.rs BY `cargo test -p suzatary-core`.
// DO NOT EDIT: docs/ARCHITECTURE.md §2 — every wire type is declared ONCE, in
// suzatary-core, and its decoder in payload.ts is folded from the same text.

/** A `to_json` node: tagged by its Term constructor, with per-constructor
 * fields a consumer renders generically. */
export ";

/// Every wire type's TypeScript, in one file, each declared once, what it names before itself.
#[must_use]
pub fn typescript() -> String {
    let mut out = String::from(HEADER);
    out.push_str(SPEC_NODE);
    out.push('\n');
    exported(&mut out, declared());
    out
}

/// The body of a generated TypeScript module, this one's and the pages'.
pub fn exported(out: &mut String, declarations: impl IntoIterator<Item = String>) {
    for decl in declarations {
        out.push_str("\nexport ");
        out.push_str(&decl);
        if !decl.ends_with(';') {
            out.push(';');
        }
        out.push('\n');
    }
}

/// `web/src/wire/generated/payload.ts`, folded from the declarations [`typescript`] prints.
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

/// The roots of [`decoders`].
#[must_use]
pub fn answer_names() -> Vec<String> {
    let mut names = Names::default();
    answers(&mut names);
    names.0
}

/// Where `value`, an answer named `name`, holds a map, whose keys are data and never fields.
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

fn declared() -> Vec<String> {
    let mut declared = Declarations::default();
    answers(&mut declared);
    bodies(&mut declared);
    declared.out
}

/// The roots of the browser's decoders: what a route answers, a stream sends or the module reads.
fn answers(visit: &mut impl TypeVisitor) {
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
    visit.visit::<RecordPayload>();
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
    visit.visit::<Overview>();
    visit.visit::<ErrorPayload>();
    visit.visit::<crate::secretary::phase::Phase>();
}

/// What a page sends: decoded by the box alone, so the browser has no decoder for one.
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

#[derive(Default)]
struct Names(Vec<String>);

impl TypeVisitor for Names {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        self.0.push(T::name());
    }
}

/// Each type's declaration after its dependencies'; `out` is what the walk declared, in order.
#[derive(Default)]
pub struct Declarations {
    seen: std::collections::BTreeSet<std::any::TypeId>,
    pub out: Vec<String>,
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

    /// `decision` alone is a decision; every other name a todo that keeps its name for the chip.
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

    /// Every answer has a decoder, folded from the text `wire.ts` holds.
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

    /// A misspelt field is a 400 and never a default.
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

    /// A phone keeps an installed APK for months, so the portal's JSON moves only with
    /// `Pairing.java`.
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
