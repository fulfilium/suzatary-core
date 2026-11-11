//! The web's payloads; their TypeScript and decoders are generated ([`shape`]). A field name is
//! the name on the wire, so renaming one is a contract change.

pub mod base;
pub mod cards;
pub mod inbox;
pub mod lines;
pub mod record;
pub mod shape;
pub mod signatures;
pub mod store;
pub mod todo;
pub mod topics;
pub mod triage;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::{Dependency, TS, TypeVisitor};

use base::ErrorPayload;
use cards::{AboutPayload, HeldTodo, Overview};
use lines::{Content, ObjectsPayload, ReceiptPayload, SkeletonPayload};
use signatures::{DevicePayload, FailedChain, Registering};
use todo::{Entry, FieldChange, Link, Neighbour, TreeNode};
use topics::{
    AttachmentRow, BlockRow, ConversationPayload, ConversationsPayload, InboxProposalsPayload,
    MemoryPayload, ProposalRow, SearchPayload, StreamPayload, TopicsPayload, TurnGone,
    UnplacedProposal,
};
use triage::WatchesPayload;

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
    pub const fn kind(self) -> crate::stores::Kind {
        use crate::stores::Kind;
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ContentPayload {
    /// One per hash asked, in the order asked.
    pub objects: Vec<Content>,
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

/// A replay of the request is answered [`RevealPayload::NotKept`], never the value.
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

/// In the order asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DrawnPayload {
    pub title: String,
    pub proposals: Vec<ProposalRow>,
}

/// `GET /api/waiting`: read on the box so a tab need not hold the conversations to show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WaitingPayload {
    /// Open proposals, open decisions (one per batch they name), and lines no record places.
    pub count: usize,
    /// By conversation; a conversation with none is absent.
    #[ts(type = "Record<string, UnplacedProposal[]>")]
    pub unplaced: BTreeMap<String, Vec<UnplacedProposal>>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SayPayload {
    pub conversation: ConversationPayload,
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
    use super::signatures::Registering;
    use super::todo::{Block, Fulfillment};
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
