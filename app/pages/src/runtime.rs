//! The effects a page's `update` returns, as data the host (`web/src/pages/program.ts`) performs,
//! and the inputs the host answers with, each tagged with the page's own `Ask`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use viewtree::kit::Reading;

use crate::zone::Offset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    Writes,
    /// It has an actor, but this device does not write as it yet.
    Reads,
    /// It names no actor: nothing can be asked of the secretary either.
    Views,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct Context {
    pub gate: Gate,
    pub reach: Reach,
    /// Oldest first, each from the instant it starts.
    pub offsets: Vec<Offset>,
    /// The tab is 62rem wide or more: a side column is drawn, and a bare Enter sends.
    pub wide: bool,
}

/// What a login that may not write is told where a box to write in would be.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Reach {
    Writes,
    /// The box has not said who it is yet: nothing is drawn.
    Waits,
    Reads {
        said: String,
        /// Whether Settings is where to fix it.
        settings: bool,
        key: Untrusted,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Untrusted {
    /// No trusted device has trusted it yet.
    Untrusted,
    Revoked,
    NotTheKey,
}

/// `H` names the page's held questions, `A` its one-time asks and acts, `S` its streams, so an
/// answer to a question of another kind does not typecheck. Holding, watching or listening under
/// an `H` already held lets the earlier one go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Effect<H, A, S = Never> {
    /// Hold a GET of `path`, handed over now and again whenever it moves, as `Input::Held`. A
    /// 4xx is the box's answer and stands; a network failure or 5xx keeps the answer that stood.
    Hold {
        ask: H,
        path: String,
    },
    Again {
        path: String,
    },
    Get {
        ask: A,
        path: String,
    },
    Post {
        ask: A,
        path: String,
        #[ts(type = "unknown")]
        body: Body,
    },
    /// Put a new key in this device's place, answered with `null`.
    NewKey {
        ask: A,
    },
    Watch {
        ask: H,
        local: Local,
    },
    /// Write a register the tab keeps; every page watching it hears the move.
    Keep {
        register: Register,
        #[ts(type = "unknown")]
        value: serde_json::Value,
    },
    /// Each frame as `Input::Heard`, and each opening of the stream, the first included, as
    /// `Input::Opened`.
    Listen {
        ask: S,
        path: String,
    },
    /// Pull every store now: what a page does when the box says something was written.
    Sync,
    /// An intent (`web/src/prodrome/intentsync.ts`): held in the tab's set until a reading of the
    /// replicas holds it, answered once with an `Outcome`.
    Act {
        ask: A,
        act: Act,
        /// The page never holds the value a typed answer carries.
        secret: Secret,
        told: Told,
    },
    /// This device's signature of the registration of `key` for `actor` (a card's endorse), read
    /// off the tab's replica of the first chain.
    Endorse {
        ask: A,
        actor: String,
        key: String,
    },
    /// Answered with the instant in epoch ms, `after` ms from now. A hidden tab's timer runs
    /// late, and a page shown again is answered at once.
    Timer {
        ask: A,
        after: u32,
    },
    /// Focus the field marked `data-field` after this step's patches, caret at the end; nothing
    /// when none stands or the keyboard is in another field.
    Focus {
        field: String,
    },
    /// Applied in the frame after this step's patches, unless the reader moved it since.
    Scroll {
        to: Scroll,
    },
    Copy {
        text: String,
    },
    Go {
        href: String,
    },
    /// Go in place of this page, so Back skips it.
    Replace {
        href: String,
    },
    Back {
        fallback: String,
    },
    Toast {
        words: String,
    },
    /// A performance mark, after the layout of this step's patches; only the latest mark of a
    /// name is kept.
    Mark {
        name: String,
    },
    /// Tell whoever mounted the page something it shows around it (the panel's toggle).
    Tell {
        #[ts(type = "unknown")]
        said: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Scroll {
    End,
    /// The element marked `data-field` at the top, below the sticky header.
    Top {
        field: String,
    },
    Into {
        field: String,
    },
    /// As far from the end as before this step's patches, so older rows drawn above do not move
    /// what was being read.
    Keep,
}

/// When an act's answer reaches the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum Told {
    Answered,
    /// Once the tab's reading holds it or says the box did not; a lost answer is read as done or
    /// not this way.
    Held,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Body {
    Empty {},
    /// `POST /api/act/key`.
    Key(suzatary_core::wire::KeyAsked),
    /// `POST /api/act/pair`.
    Pair(suzatary_core::wire::Pairing),
    /// `POST /api/secretary/conversations` and `…/title`.
    Titling(suzatary_core::wire::Titling),
    /// `POST /api/secretary/attachments`.
    Upload(suzatary_core::wire::Uploading),
}

/// A reading the tab keeps of its own, which no GET answers; each names the payload it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Local {
    /// `null` when the browser answers nothing.
    Device,
    /// Priced through every chain the tab holds (`InboxRead`).
    Proposals,
    /// As the box last folded it (`FoldPayload`).
    Decisions {
        chain: String,
    },
    /// Newest first (`ConversationsPayload`).
    Conversations,
    Search {
        q: String,
    },
    /// The newest `pages` pages of rounds carrying every topic of `chosen` (`StreamRead`).
    Stream {
        /// Each topic once, sorted.
        chosen: Vec<String>,
        pages: u32,
    },
    /// Grouped into rounds on the owner's days (`Windowed`).
    Window {
        id: String,
        window: Window,
    },
    Cards {
        conversation: String,
    },
    /// What this tab asked and the replicas do not hold yet (`Intents`).
    Intents,
    Titles,
    /// A composer's unsent words (`Draft`), synced to the owner's other devices.
    Draft {
        /// The conversation, or the new topic's key.
        id: String,
    },
    /// Any value, `null` before one is kept.
    Ui {
        key: String,
    },
    Scroll,
    Route,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Register {
    /// Value `{"type": "Typed", "text"}` as typed, or `{"type": "Settled", "text"}` taking those
    /// words over a conflict.
    Draft {
        id: String,
    },
    Ui {
        key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
pub struct Scrolled {
    pub(crate) by: Mover,
    pub(crate) at: Place,
}

/// `Page` covers its own placing, content shrinking under the box, and the browser's scroll
/// anchoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mover {
    Reader,
    Page,
}

/// `Bottom` is within a screen of the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Place {
    Bottom,
    Above,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Window {
    /// At least the core's own window.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub before: Option<u32>,
    /// A reference every round it holds touches.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub about: Option<String>,
}

/// An act as the tab's set of intents names it (`web/src/prodrome/asks.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Act {
    Answer {
        at: Asked,
        answer: Answering,
    },
    Question {
        kind: Questioning,
        question: Question,
    },
    Round {
        kind: Rounding,
        conversation: String,
        after: u32,
    },
    Rename {
        conversation: String,
        title: String,
    },
    Forget {
        fact: suzatary_core::secretary::memory::FactId,
    },
    Erase {
        conversation: String,
        turns: Vec<u32>,
    },
    /// Sealed into the tab's replica of the topic store.
    Topic {
        write: TopicWrite,
    },
}

/// A proposal as its card saw it: `from` is its signature then, and any later climb of its
/// machine holds the answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Asked {
    pub(crate) conversation: String,
    pub(crate) index: u32,
    pub from: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Answering {
    Tap {
        note: String,
        /// The wider yes.
        always: bool,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        #[ts(optional)]
        endorsement: Option<Endorsement>,
    },
    Dismiss {
        reason: String,
    },
    Acknowledge,
    Drop {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Endorsement {
    pub(crate) key: String,
    pub(crate) signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Questioning {
    Say,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Rounding {
    Stop,
    Regenerate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Question {
    pub(crate) conversation: String,
    pub(crate) after: u32,
    pub(crate) text: String,
    pub(crate) references: Vec<String>,
    /// Already kept on the box.
    pub(crate) files: Vec<suzatary_core::wire::AttachmentRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum TopicWrite {
    Add { target: String, topic: String },
    Remove { target: String, topic: String },
    Rename { from: String, to: String },
}

/// The value a typed answer carries, which the page never holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Secret {
    None,
    /// The field marked `data-field`, read at the act and kept by the tab under `keep` to be sent
    /// again.
    Field {
        field: String,
        keep: String,
    },
    /// The value kept under `kept`, sent once more and let go; the act is not asked when none is
    /// kept.
    Kept {
        kept: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Outcome {
    Already,
    Answered { answer: Said, held: bool },
}

/// `intents.Answer`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Said {
    Taken,
    Refused {
        e: String,
    },
    /// No answer came: it may or may not have reached the box.
    Lost {
        e: String,
    },
}

/// The `A` of a page that asks nothing once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Never {}

/// Whether a held question's answer may be behind the box (the tab's `reading.Behind`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
pub struct Behind {
    /// Why the last ask failed, or `None` while one is long on its way.
    pub why: Option<String>,
    /// Epoch milliseconds.
    #[ts(type = "number")]
    pub since: i64,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Answer {
    Done {
        #[ts(type = "unknown")]
        body: serde_json::Value,
    },
    Refused {
        /// 0 when no answer came.
        status: u16,
        words: String,
    },
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Input<H, A, S = Never> {
    Context(Context),
    Held {
        ask: H,
        reading: Reading<Answer>,
        behind: Option<Behind>,
    },
    Answered {
        ask: A,
        answer: Answer,
    },
    Heard {
        ask: S,
        frame: String,
    },
    Opened {
        ask: S,
    },
}

/// A refusal, or an answer of another shape, is an error in its words.
#[must_use]
pub(crate) fn read<T: DeserializeOwned>(r: Reading<Answer>) -> Reading<T> {
    match r {
        Reading::Loading => Reading::Loading,
        Reading::Error { e } => Reading::Error { e },
        Reading::Val { v } => match answered(v) {
            Ok(v) => Reading::Val { v },
            Err(e) => Reading::Error { e },
        },
    }
}

/// # Errors
/// The box refused it, or answered in another shape.
pub(crate) fn answered<T: DeserializeOwned>(a: Answer) -> Result<T, String> {
    match a {
        Answer::Done { body } => serde_json::from_value(body)
            .map_err(|e| format!("the box answered in a shape this page does not read ({e})")),
        Answer::Refused { words, .. } => Err(words),
    }
}

/// `s` as `encodeURIComponent` spells it, so a path the page prints matches the TypeScript's.
#[must_use]
pub(crate) fn component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xf)]));
        }
    }
    out
}

/// `?name=value&…` of the non-empty fields, each value encoded; `""` for none.
#[must_use]
pub(crate) fn search(fields: &[(&str, &str)]) -> String {
    let pairs: Vec<String> = fields
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(n, v)| format!("{n}={}", component(v)))
        .collect();
    if pairs.is_empty() {
        String::new()
    } else {
        format!("?{}", pairs.join("&"))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Held<T> {
    pub(crate) reading: Reading<T>,
    pub(crate) behind: Option<Behind>,
}

impl<T> Held<T> {
    #[must_use]
    pub(crate) const fn loading() -> Self {
        Held {
            reading: Reading::Loading,
            behind: None,
        }
    }
}

impl<T: DeserializeOwned> Held<T> {
    #[must_use]
    pub(crate) fn of(reading: Reading<Answer>, behind: Option<Behind>) -> Self {
        Held {
            reading: read(reading),
            behind,
        }
    }
}
