//! One line of a conversation, and the instant every line carries.

use super::{Begun, Decision, Kind, Namer, Scope, Start, Title};
use crate::secretary::model::Turn;
use serde::Serialize;
use suzatary_base::capability::Datetime;
use suzatary_connect::connect::TemplateId;
use suzatary_inbox::secretary::stamp::Stamp;
use suzatary_memory::secretary::locked_text;
use suzatary_memory::secretary::words::Words;

/// One line of a conversation as the box reads it from its store; its JSON print is for a test's
/// record of a scene, and nothing reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "line", rename_all = "lowercase")]
pub enum Entry<T = Turn> {
    /// The first line of every file, naming the model behind it.
    Opened {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        title: Words<String>,
        model: String,
        kind: Kind,
    },
    /// A conversation begun on a template's opening: a topic titled by the template.
    SetUp {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        title: Words<String>,
        #[serde(serialize_with = "template_name")]
        template: TemplateId,
    },
    /// The conversation was named; the last one wins.
    Titled {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        title: Words<Title>,
        by: Namer,
    },
    Said {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        turn: T,
        /// Who stamped it: the actor the gate admitted a `say` or an act as, or the box for a
        /// note of its own; `None` for words the box relays, which `secretary::taint` reads as
        /// "not the owner".
        #[serde(skip_serializing_if = "Option::is_none")]
        by: Option<Stamp>,
    },
    /// The secretary proposed an outward act; its index among these lines is what a tap names, and
    /// the act and what became of it are the inbox's.
    Proposed {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        object: suzatary_inbox::secretary::inbox::ProposalId,
        /// The earlier proposal this one replaces.
        #[serde(skip_serializing_if = "Option::is_none")]
        revises: Option<usize>,
    },
    /// The secretary withdrew, or deliberately left, one of the triage agent's cards.
    Reviewed {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        chain: String,
        /// What was reviewed: a card's reference, or an older batch's head.
        batch: String,
        #[serde(flatten)]
        decision: Decision,
        /// Why, in the secretary's own words; never empty.
        why: Words<String>,
    },
    /// The review was asked for and could not run.
    ReviewFailed {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The box's own words for why — the model's refusal, verbatim.
        reason: Words<String>,
        /// How many cards it left open at that moment.
        standing: usize,
    },
    /// The owner stopped the running round, written once its model process ended so nothing lands
    /// after it; it closes the question it stopped, since a killed process holds a question it
    /// never answered.
    Stopped {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// Who stopped it: the actor the gate admitted.
        by: String,
    },
    /// The last round was asked again: every turn from `to` on is set aside, kept and drawn as an
    /// earlier attempt but never delivered to a model again; only
    /// [`Stored::rewind`](super::Stored::rewind) decides `to`.
    Rewound {
        /// The first turn set aside.
        to: usize,
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// Who asked again: the actor the gate admitted.
        by: String,
    },
    /// A line of retired bookkeeping, read and never written.
    Retired {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        #[serde(flatten)]
        retired: super::Retired,
    },
    /// How the asks from here on send their model this conversation, recorded when sent.
    Scoped {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        scope: Scope,
    },
    /// A coding session a tapped proposal started, as it now stands; the last line for an index is
    /// its state.
    Coding {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The proposal that started it, by index.
        proposal: usize,
        session: suzatary_inbox::coding::Session,
    },
    /// Lines of this conversation whose words were erased, named by their keys: every reader
    /// reads those slots as gone, saying why. It says nothing of its own, so no turn's index
    /// moves.
    Erased {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        keys: std::collections::BTreeSet<locked_text::KeyName>,
        erasure: suzatary_memory::secretary::erasure::Erasure,
    },
    /// One slot's words sealed again, scrubbed under the current redaction rules, under a key of
    /// its own: every reader reads the slot `of` names from it once the key it was sealed from is
    /// destroyed. It says nothing of its own, so no turn's index moves.
    Resealed {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The key of the slot as first written, which names the slot.
        of: locked_text::KeyName,
        /// The key of the seal it was sealed again from, which it supersedes.
        from: locked_text::KeyName,
    },
    /// A person asked again: every live turn beneath this line that descends from `question` is
    /// set aside, and `question` too for an edit, each named by descent and never by index, so a
    /// line concurrent with it is never set aside.
    Again {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The question asked again, by its line's name.
        #[serde(serialize_with = "line_name")]
        question: prodrome_data::event::Hash,
        how: super::Rewind,
        /// Who asked again: the line's writer.
        by: String,
    },
    /// The box claims a round: run `run` covers these turns, each with the taint the box judged
    /// it to carry. It says nothing of its own, so no turn's index moves.
    Began {
        #[serde(serialize_with = "stamped")]
        at: Datetime,
        /// The turns it adds to its round, in order.
        covers: Vec<super::Covered>,
        run: super::RunId,
    },
}

impl<T> Entry<T> {
    /// The title this line gives its conversation: an opening's, or a naming's, which the last
    /// naming in order replaces; `None` for a line that names nothing.
    #[must_use]
    pub fn title(&self) -> Option<Words<String>> {
        match self {
            Entry::Opened { title, .. } | Entry::SetUp { title, .. } => Some(title.clone()),
            Entry::Titled { title, .. } => Some(named(title)),
            Entry::Said { .. }
            | Entry::Proposed { .. }
            | Entry::Reviewed { .. }
            | Entry::ReviewFailed { .. }
            | Entry::Stopped { .. }
            | Entry::Rewound { .. }
            | Entry::Retired { .. }
            | Entry::Scoped { .. }
            | Entry::Coding { .. }
            | Entry::Erased { .. }
            | Entry::Resealed { .. }
            | Entry::Began { .. }
            | Entry::Again { .. } => None,
        }
    }

    #[must_use]
    pub fn at(&self) -> Datetime {
        match self {
            Entry::Opened { at, .. }
            | Entry::SetUp { at, .. }
            | Entry::Titled { at, .. }
            | Entry::Said { at, .. }
            | Entry::Proposed { at, .. }
            | Entry::Reviewed { at, .. }
            | Entry::ReviewFailed { at, .. }
            | Entry::Stopped { at, .. }
            | Entry::Rewound { at, .. }
            | Entry::Retired { at, .. }
            | Entry::Scoped { at, .. }
            | Entry::Coding { at, .. }
            | Entry::Erased { at, .. }
            | Entry::Resealed { at, .. }
            | Entry::Began { at, .. }
            | Entry::Again { at, .. } => *at,
        }
    }
}

impl<T> Entry<T> {
    /// What this line begins, where it begins a conversation.
    #[must_use]
    pub fn start(&self) -> Option<Start<'_>> {
        match self {
            Entry::Opened {
                at,
                title,
                model,
                kind,
            } => Some(Start {
                at: *at,
                title,
                kind: *kind,
                begun: Begun::Opened { model },
            }),
            Entry::SetUp {
                at,
                title,
                template,
            } => Some(Start {
                at: *at,
                title,
                kind: Kind::Topic,
                begun: Begun::SetUp(*template),
            }),
            Entry::Titled { .. }
            | Entry::Said { .. }
            | Entry::Proposed { .. }
            | Entry::Reviewed { .. }
            | Entry::ReviewFailed { .. }
            | Entry::Stopped { .. }
            | Entry::Rewound { .. }
            | Entry::Retired { .. }
            | Entry::Scoped { .. }
            | Entry::Coding { .. }
            | Entry::Erased { .. }
            | Entry::Resealed { .. }
            | Entry::Began { .. }
            | Entry::Again { .. } => None,
        }
    }
}

fn stamped<S: serde::Serializer>(at: &Datetime, to: S) -> Result<S::Ok, S::Error> {
    to.serialize_str(&suzatary_inbox::secretary::stamp::stamp(*at))
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's serialize_with hands a reference"
)]
fn template_name<S: serde::Serializer>(template: &TemplateId, to: S) -> Result<S::Ok, S::Error> {
    to.serialize_str(template.as_str())
}

pub(super) fn named(title: &Words<Title>) -> Words<String> {
    title.clone().map(String::from)
}

fn line_name<S: serde::Serializer>(
    name: &prodrome_data::event::Hash,
    to: S,
) -> Result<S::Ok, S::Error> {
    to.serialize_str(name.as_str())
}
