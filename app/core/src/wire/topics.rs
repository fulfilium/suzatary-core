use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::inbox::{Asker, Card, CodingSession, ProposedAct};
use super::signatures::FailedChain;
use super::todo::Fulfillment;
use suzatary_base::reference::Reference;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BlockRow {
    /// Its typst with the blank line that ends it.
    pub source: String,
    pub plain: String,
    pub kind: suzatary_base::answer::Kind,
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
    pub asked: suzatary_base::reference::ProposalRef,
    #[ts(type = "string | null")]
    pub serves: Option<suzatary_base::reference::TodoRef>,
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

/// `GET /api/inbox`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct InboxProposalsPayload {
    pub at: String,
    /// Oldest first.
    pub proposals: Vec<InboxProposal>,
    pub failed: Vec<FailedChain>,
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
