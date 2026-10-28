//! One line of a conversation, and the instant every line carries.

use super::{Decision, Kind, Namer, Scope, Title};
use crate::capability::{Datetime, SessionId, Turn};
use crate::reference::Reference;
use crate::secretary::stamp::Stamp;
use crate::secretary::words::Words;
use serde::Serialize;

/// One line of a conversation as the box reads it from its store. It prints as JSON tagged on
/// `line`, each instant as its stamp, for a test's record of a scene; nothing reads that print.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "line", rename_all = "lowercase")]
pub enum Entry {
    /// The first line of every file: when it was opened, what about, and which model is behind it,
    /// which a reader of an old conversation cannot otherwise recover.
    Opened {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// What it is about, for the list.
        title: Words<String>,
        /// The model's own name for itself ([`crate::capability::Model::name`]).
        model: String,
        /// What it is for.
        kind: Kind,
    },
    /// The conversation was named; the last one wins, so a rename is an append.
    Titled {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The name.
        title: Words<Title>,
        /// Who gave it.
        by: Namer,
    },
    /// Something was said: by the owner, by the model, or by a verb.
    Said {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The turn.
        turn: Turn,
        /// Who stamped it: the actor the gate admitted a `say` or an act as, or the box for a
        /// note of its own; `None` for words the box relays, which `secretary::taint` reads as
        /// "not the owner".
        #[serde(skip_serializing_if = "Option::is_none")]
        by: Option<Stamp>,
    },
    /// The secretary proposed an outward act; its index among these lines is what a tap names, and
    /// the act and what became of it are the inbox's.
    Proposed {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The inbox object: the act, its reasons and its responses.
        object: crate::secretary::inbox::ProposalId,
        /// The earlier proposal this one replaces, as the secretary wrote it; the inbox records
        /// the replaced card as superseded.
        #[serde(skip_serializing_if = "Option::is_none")]
        revises: Option<usize>,
    },
    /// The secretary withdrew, or deliberately left, one of the triage agent's cards.
    ///
    /// A decision nobody can read afterwards is not delegated, so the briefing,
    /// `suzatary secretary reviews` and the Inbox all read this record.
    Reviewed {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// Which chain the card was against.
        chain: String,
        /// What was reviewed: a card's reference, or an older batch's head.
        batch: String,
        /// What was decided, and the objects it settled.
        #[serde(flatten)]
        decision: Decision,
        /// Why, in the secretary's own words; never empty, since the reason is the whole of what
        /// the owner reads back.
        why: Words<String>,
    },
    /// The review was asked for and could not run.
    ///
    /// Recorded like a failed send, so an unsettled Inbox is distinguishable from one with nothing
    /// to settle. Appended after the cards were read in, so a reader sees what stood at that
    /// moment.
    ReviewFailed {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The box's own words for why — the model's refusal, verbatim.
        reason: Words<String>,
        /// How many cards it left open, recorded then because what stands now is a different
        /// number.
        standing: usize,
    },
    /// The owner stopped the running round, written once its model process ended so nothing lands
    /// after it. [`Stored::of`](super::Stored::of) reads it as an act ([`stopped_by`](super::stopped_by)) that [closes](super::Stored::closes)
    /// the question it stopped, and as the end of the session, since a killed process holds a
    /// question it never answered.
    Stopped {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// Who stopped it: the actor the gate admitted.
        by: String,
    },
    /// The last round was asked again: every turn from `to` on is set aside, kept and drawn as an
    /// earlier attempt but never delivered to a model again. A regenerate keeps the question
    /// pending; an edit sets it aside too. [`Stored::of`](super::Stored::of) drops the session; only
    /// [`Stored::rewind`](super::Stored::rewind) decides `to`.
    Rewound {
        /// The first turn set aside.
        to: usize,
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// Who asked again: the actor the gate admitted.
        by: String,
    },
    /// This conversation has been told the memory as of `version`, written once a batch headed with
    /// the memory's change was answered. A session opened fresh is told everything regardless.
    MemorySeen {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The memory's version it was told.
        version: u64,
    },
    /// A model session stands behind this conversation from here, written when one is first
    /// opened and whenever cold recovery mints a fresh one after
    /// [`crate::capability::ModelError::Lost`]. [`Stored::of`](super::Stored::of) keeps the last: a register with one
    /// writer.
    Session {
        /// When this line was written.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The session id `--session-id`/`--resume` names.
        session: SessionId,
        /// The model it was opened with.
        model: String,
        /// The directory it was opened in.
        cwd: String,
        /// The manual-and-verbs digest it was opened with; absent on older lines.
        #[serde(skip_serializing_if = "Option::is_none")]
        surface: Option<String>,
    },
    /// A session opened over this conversation's turns from `from` on: what the ask that opened
    /// it sent as turns, recorded as the fact it was when it was sent, never derived again from
    /// the turns that stand when this line is read.
    Kept {
        /// When this line was written.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The session id `--session-id`/`--resume` names.
        session: SessionId,
        /// The model it was opened with.
        model: String,
        /// The directory it was opened in.
        cwd: String,
        /// The manual-and-verbs digest it was opened with.
        #[serde(skip_serializing_if = "Option::is_none")]
        surface: Option<String>,
        /// The first turn it was sent.
        from: usize,
    },
    /// A turn was said past the kept `session`: a model holding no session was sent the whole
    /// window and answered as `model`, or the box ended the exchange. A session is sent only the
    /// turns since its last, so [`Stored::of`](super::Stored::of) ends that session, and the next held round opens
    /// one with the whole window.
    Unheard {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The session that did not hear it.
        session: SessionId,
        /// Who said it.
        model: String,
    },
    /// How the asks from here on send their model this conversation, written by the first ask that
    /// sends it otherwise than the conversation's last [`Scope`](super::Scope): a fact of the ask, recorded when
    /// it is sent, so what its model holds is read and never guessed from the conversation's kind.
    Scoped {
        /// When this line was written.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// What the ask sent.
        scope: Scope,
    },
    /// A coding session a tapped proposal started, as it now stands; the last line for an index is
    /// its state.
    Coding {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The proposal that started it, by index.
        proposal: usize,
        /// The session.
        session: crate::coding::Session,
    },
    /// Lines of this conversation whose words were erased, named by their keys: every reader
    /// reads those slots as gone, saying why. It says nothing of its own, so no turn's index
    /// moves.
    Erased {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The keys whose words went.
        keys: std::collections::BTreeSet<crate::secretary::locked_text::KeyName>,
        /// Why.
        erasure: crate::secretary::erasure::Erasure,
    },
    /// One slot's words sealed again, scrubbed under the current redaction rules, under a key of
    /// its own: every reader reads the slot `of` names from it once the key it was sealed from is
    /// destroyed. It says nothing of its own, so no turn's index moves.
    Resealed {
        /// When.
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The key of the slot as first written, which names the slot.
        of: crate::secretary::locked_text::KeyName,
        /// The key of the seal it was sealed again from, which it supersedes.
        from: crate::secretary::locked_text::KeyName,
    },
}

impl Entry {
    /// The title this line gives its conversation: an opening's, or a naming's, which the last
    /// naming in order replaces; `None` for a line that names nothing.
    #[must_use]
    pub fn title(&self) -> Option<Words<String>> {
        match self {
            Entry::Opened { title, .. } => Some(title.clone()),
            Entry::Titled { title, .. } => Some(named(title)),
            Entry::Said { .. }
            | Entry::Proposed { .. }
            | Entry::Reviewed { .. }
            | Entry::ReviewFailed { .. }
            | Entry::Stopped { .. }
            | Entry::Rewound { .. }
            | Entry::MemorySeen { .. }
            | Entry::Session { .. }
            | Entry::Kept { .. }
            | Entry::Unheard { .. }
            | Entry::Scoped { .. }
            | Entry::Coding { .. }
            | Entry::Erased { .. }
            | Entry::Resealed { .. } => None,
        }
    }

    /// When it was written: every line carries an instant.
    #[must_use]
    pub fn at(&self) -> Datetime {
        match self {
            Entry::Opened { at, .. }
            | Entry::Titled { at, .. }
            | Entry::Said { at, .. }
            | Entry::Proposed { at, .. }
            | Entry::Reviewed { at, .. }
            | Entry::ReviewFailed { at, .. }
            | Entry::Stopped { at, .. }
            | Entry::Rewound { at, .. }
            | Entry::MemorySeen { at, .. }
            | Entry::Session { at, .. }
            | Entry::Kept { at, .. }
            | Entry::Unheard { at, .. }
            | Entry::Scoped { at, .. }
            | Entry::Coding { at, .. }
            | Entry::Erased { at, .. }
            | Entry::Resealed { at, .. } => *at,
        }
    }
}

impl Entry {
    /// The box's own act line: words it composed from its own state (a proposal's number, a
    /// key's name, a restart, the owner's note on his own tap), carrying nobody else's, stamped
    /// [`Stamp::Box`] so taint reads it clean and no round counts it the owner's.
    #[must_use]
    pub fn noted(at: Datetime, text: String, references: Vec<Reference>) -> Self {
        Entry::Said {
            at,
            turn: Turn::Act { text, references },
            by: Some(Stamp::Box),
        }
    }
}

/// The `at` every stored line carries, the wire's spelling of an instant, so a transcript and a
/// payload date a thing the same way.
#[must_use]
pub fn stamp(at: Datetime) -> String {
    prodrome_core::fpl::iso(prodrome_core::fpl::instant_of(at))
}

/// An instant as [`stamp`] prints it, for [`Entry`]'s print.
fn stamped<S: serde::Serializer>(at: &Datetime, to: S) -> Result<S::Ok, S::Error> {
    to.serialize_str(&stamp(*at))
}

/// A naming's title, as the conversation holds it.
pub(super) fn named(title: &Words<Title>) -> Words<String> {
    title.clone().map(String::from)
}
