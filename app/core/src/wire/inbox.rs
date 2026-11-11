//! The inbox's wire.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// In prose for somebody deciding, where [`FieldChange`](super::todo::FieldChange) carries the
/// canonical print as evidence.
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
        style: crate::secretary::proposal::Style,
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
    Restore {
        card: String,
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
