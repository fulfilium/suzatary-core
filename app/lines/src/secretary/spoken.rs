//! The lines the box writes whole, and the turn they hold: none is a model's turn, which is
//! written as an [`Answer`](crate::secretary::answer::Answer) with what its call was sent.

use std::borrow::Borrow;
use std::convert::Infallible;

use suzatary_base::capability::Datetime;
use suzatary_base::reference::Reference;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::outcome::Outcome;
use suzatary_conversations::secretary::store::Entry;
use suzatary_conversations::secretary::tool_name::ToolName;
use suzatary_held::attachment::Attachment;
use suzatary_held::enclosed::Enclosed;
use suzatary_inbox::secretary::own_round::OwnRound;
use suzatary_inbox::secretary::stamp::Stamp;

/// A person's turn, a verb's answer or an act: a [`Turn`] whose constructors make no model's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spoken(Turn);

/// A line with no model's turn.
pub type Plain = Entry<Spoken>;

impl Spoken {
    #[must_use]
    pub fn person(text: String, references: Vec<Reference>, attachments: Vec<Attachment>) -> Self {
        Self(Turn::Person {
            text,
            references,
            attachments,
        })
    }

    #[must_use]
    pub fn tool(verb: ToolName, answer: String, outcome: Outcome, files: Vec<Enclosed>) -> Self {
        Self(Turn::Tool {
            verb,
            answer,
            outcome,
            files,
        })
    }

    #[must_use]
    pub fn act(text: String, references: Vec<Reference>) -> Self {
        Self(Turn::Act { text, references })
    }
}

impl Borrow<Turn> for Spoken {
    fn borrow(&self) -> &Turn {
        &self.0
    }
}

impl From<Spoken> for Turn {
    fn from(spoken: Spoken) -> Self {
        spoken.0
    }
}

/// The box's own act line: words it composed from its own state, carrying nobody else's, stamped
/// [`Stamp::Box`] so taint reads it clean and no round counts it the owner's.
#[must_use]
pub fn noted(at: Datetime, text: String, references: Vec<Reference>) -> Plain {
    Entry::Said {
        at,
        turn: Spoken::act(text, references),
        by: Some(Stamp::Box),
    }
}

/// The question of one of the box's own rounds, stamped with the round it opens.
#[must_use]
pub fn asked_by_the_box(
    at: Datetime,
    round: OwnRound,
    text: String,
    references: Vec<Reference>,
) -> Plain {
    Entry::Said {
        at,
        turn: Spoken::person(text, references, Vec::new()),
        by: Some(Stamp::Asking(round)),
    }
}

/// `entry` with its turn changed by `f`, where it says one; every other line is as it was.
///
/// # Errors
/// What `f` refuses its turn with.
pub fn try_map_turn<T, U, E>(
    entry: Entry<T>,
    f: impl FnOnce(T) -> Result<U, E>,
) -> Result<Entry<U>, E> {
    /// The lines that hold no turn, each carried over as it is.
    macro_rules! unchanged {
        ($($line:ident { $($field:ident),* },)*) => {
            match entry {
                Entry::Said { at, turn, by } => Entry::Said { at, turn: f(turn)?, by },
                $(Entry::$line { $($field),* } => Entry::$line { $($field),* },)*
            }
        };
    }
    Ok(unchanged! {
        Opened { at, title, model, kind },
        SetUp { at, title, template },
        Titled { at, title, by },
        Proposed { at, object, revises },
        Reviewed { at, chain, batch, decision, why },
        ReviewFailed { at, reason, standing },
        Stopped { at, by },
        Rewound { to, at, by },
        Retired { at, retired },
        Scoped { at, scope },
        Coding { at, proposal, session },
        Erased { at, keys, erasure },
        Resealed { at, of, from },
        Again { at, question, how, by },
        Began { at, covers, run },
    })
}

/// `plain` as an entry that may hold any turn, as a reader takes one.
#[must_use]
pub fn widened(plain: Plain) -> Entry {
    let Ok(entry) = try_map_turn(plain, |spoken| Ok::<_, Infallible>(Turn::from(spoken)));
    entry
}
