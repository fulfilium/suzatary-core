//! The digest's schema: what was said of one node of the conversations' summary tree, keyed by
//! the node's content name.
//!
//! A summary is an observation a model made of the lines a node spans, never a function of them,
//! so it is an object: `Summarized` names its words by their [`TextName`] (the words live in the
//! digest's own text store, box-local and erasable) and the objects it was written from; and
//! `Retracted` says a summary is wrong, so the node reads as unsummarized and is offered again.
//! A summary records the taint of the words it was written from, read off the reading by the
//! one writer and never told by the model; an attempt whose every answer was too long is
//! `Unfit`, so the node stays unsummarized and says why, and no cut line is ever stored as its
//! summary. Every call to a model is an `Asked`, appended BEFORE the call is made, so what the
//! compactor spent is a fold over these objects that no failure can leave uncounted; an `Asked`
//! writes no register, so it never stands as a node's reading.
//! Only a [`Tap`] makes a `Retracted`, and it is about standing (`Schema::asks`), so one
//! written as anyone but an owner claims and moves nothing ([`crate::secretary::digest_policy`]).
//! Every other event writes the node's one register, discrete ([`Sole`]):
//! two summaries no later write descends from are a conflict, never settled by time.

use prodrome_core::event::{Actor, Hash};
use prodrome_core::fold::Frontier;
use prodrome_core::literal::{Datetime, ProdromeError, Value};
use prodrome_core::registers::Stamp;

use crate::envelope::{Event, Says, Vocab};
use crate::literal::{Fields, Literal as _, Signature};
use crate::sole::{One, Only, Sole};

use crate::model_name::ModelName;
use crate::secretary::digest_line::{Line, NODE_BYTES};
use crate::secretary::salted_text::{Text, TextName};
use crate::secretary::taint::Taint;
use crate::secretary::tap::Tap;

/// One event about one node: which, when, who wrote it, and what it says.
pub type DigestEvent = Event<Said>;

/// What an event says of its node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// A summary was written: its words by name, what it was written from, by which model, and
    /// its length.
    Summarized {
        /// The name of its words in the digest's text store.
        text: TextName,
        /// What it was written from, in the log's order: each child's line, or the summary of
        /// it that was read.
        over: Vec<Hash>,
        /// The model that wrote it.
        model: ModelName,
        /// Its words' length in bytes, at most [`NODE_BYTES`]: what it costs in a view.
        bytes: u32,
        /// The node's taint when it was written: the join of the lines it spans, so tainted
        /// whenever any of them is.
        taint: Taint,
    },
    /// No answer of an attempt was a line of at most `limit` bytes, so the node has no summary
    /// from `model`: it stays unsummarized, and this says why. Keyed by both, so another model,
    /// or a larger [`NODE_BYTES`], is owed the node again.
    Unfit {
        /// The model asked.
        model: ModelName,
        /// The most bytes a line could hold when it was asked: [`NODE_BYTES`] then.
        limit: u32,
        /// The shortest answer's length in bytes, once its spaces were joined.
        shortest: u32,
    },
    /// A call to the model about this node, of `sent` bytes, recorded before it was made.
    Asked {
        /// The bytes sent: the whole system prompt and question.
        sent: u64,
    },
    /// The node's summary is wrong: it reads as having none, and may be written again.
    Retracted,
}

/// An event that cannot be.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotADigestEvent {
    /// A summary written from nothing.
    #[error("a summary is written from at least one line or summary")]
    FromNothing,
    /// A length no [`Line`] has.
    #[error("a summary's length is 1 to {NODE_BYTES} bytes, got {0}")]
    Length(u32),
}

/// How a summary came to be written: from what, by which model, at what taint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    /// What it was written from, in the log's order.
    pub over: Vec<Hash>,
    /// The model that wrote it.
    pub model: ModelName,
    /// The node's taint.
    pub taint: Taint,
}

impl DigestEvent {
    /// Node `node` summarized by `actor` as `text`, as `attempt` says. The name and length are
    /// the text's own, so an event cannot name one text and price another.
    ///
    /// # Errors
    /// [`NotADigestEvent::FromNothing`] for an attempt over nothing.
    pub fn summarized(
        node: Hash,
        at: Datetime,
        actor: Actor,
        text: &Text<Line>,
        attempt: Attempt,
    ) -> Result<Self, NotADigestEvent> {
        let bytes = u32::try_from(text.words().as_str().len())
            .map_err(|_| NotADigestEvent::Length(u32::MAX))?;
        let Attempt { over, model, taint } = attempt;
        Self::checked(
            node,
            at,
            actor,
            Said::Summarized {
                text: text.name(),
                over,
                model,
                bytes,
                taint,
            },
        )
    }

    /// Node `node` left unsummarized by `actor`'s attempt with `model`, whose shortest answer was
    /// `shortest` bytes, under today's [`NODE_BYTES`].
    #[must_use]
    pub fn unfit(node: Hash, at: Datetime, actor: Actor, model: ModelName, shortest: u32) -> Self {
        let limit = u32::try_from(NODE_BYTES).unwrap_or(u32::MAX);
        Self::of(
            node,
            at,
            actor,
            Said::Unfit {
                model,
                limit,
                shortest,
            },
        )
    }

    /// A call of `sent` bytes about `node` that `actor` is about to make.
    #[must_use]
    pub fn asked(node: Hash, at: Datetime, actor: Actor, sent: u64) -> Self {
        Self::of(node, at, actor, Said::Asked { sent })
    }

    /// Node `node`'s summary retracted by whoever tapped `tap`: the actor is the gate's
    /// mapping of the peer, never an argument.
    #[must_use]
    pub fn retracted(node: Hash, at: Datetime, tap: &Tap) -> Self {
        Self::of(node, at, tap.who().clone(), Said::Retracted)
    }

    /// The one door both the constructors and the parse go through.
    fn checked(
        node: Hash,
        at: Datetime,
        actor: Actor,
        said: Said,
    ) -> Result<Self, NotADigestEvent> {
        match &said {
            Said::Summarized { over, .. } if over.is_empty() => {
                return Err(NotADigestEvent::FromNothing);
            }
            Said::Summarized { bytes, .. }
                if !(1..=NODE_BYTES).contains(&usize::try_from(*bytes).unwrap_or(usize::MAX)) =>
            {
                return Err(NotADigestEvent::Length(*bytes));
            }
            Said::Summarized { .. } | Said::Unfit { .. } | Said::Asked { .. } | Said::Retracted => {
            }
        }
        Ok(Self::of(node, at, actor, said))
    }

    /// The bytes it sent, if it is a call.
    #[must_use]
    pub fn sent(&self) -> Option<u64> {
        match self.said() {
            Said::Asked { sent } => Some(*sent),
            Said::Summarized { .. } | Said::Unfit { .. } | Said::Retracted => None,
        }
    }
}

/// `Summarized(node, at, actor, text, over, model, bytes, taint)`.
const SUMMARIZED: Signature = (
    "Summarized",
    &[
        "node", "at", "actor", "text", "over", "model", "bytes", "taint",
    ],
);
/// `Unfit(node, at, actor, model, limit, shortest)`.
const UNFIT: Signature = (
    "Unfit",
    &["node", "at", "actor", "model", "limit", "shortest"],
);
/// `Asked(node, at, actor, sent)`.
const ASKED: Signature = ("Asked", &["node", "at", "actor", "sent"]);
/// `Retracted(node, at, actor)`.
const RETRACTED: Signature = ("Retracted", &["node", "at", "actor"]);
/// The constructors, in print order.
const SIGNATURES: &[Signature] = &[SUMMARIZED, UNFIT, ASKED, RETRACTED];

/// The digest's whitelist: its constructors and nothing else, since no event nests a term.
pub type DigestVocabulary = Vocab<Said>;

impl Says for Said {
    type Key = Hash;
    type Register = Only;
    type Registers<'a> = One<'a, DigestEvent>;

    const EVENTS: &'static [Signature] = SIGNATURES;

    fn registers() -> Vec<Only> {
        vec![Only]
    }

    /// A call writes no register: it is counted, and never a node's reading.
    fn writes(event: &DigestEvent) -> impl Iterator<Item = Only> {
        (!matches!(event.said(), Said::Asked { .. }))
            .then_some(Only)
            .into_iter()
    }

    /// A retraction is about standing ([`crate::secretary::digest_policy`]); an attempt binds
    /// whoever wrote it.
    fn asks(event: &DigestEvent) -> bool {
        matches!(event.said(), Said::Retracted)
    }

    fn body(&self) -> (Signature, Vec<Value>) {
        match self {
            Said::Summarized {
                text,
                over,
                model,
                bytes,
                taint,
            } => (
                SUMMARIZED,
                vec![
                    text.to_value(),
                    over.to_value(),
                    model.to_value(),
                    bytes.to_value(),
                    taint.to_value(),
                ],
            ),
            Said::Unfit {
                model,
                limit,
                shortest,
            } => (
                UNFIT,
                vec![model.to_value(), limit.to_value(), shortest.to_value()],
            ),
            Said::Asked { sent } => (ASKED, vec![sent.to_value()]),
            Said::Retracted => (RETRACTED, Vec::new()),
        }
    }

    fn parse(fields: &Fields<'_>) -> Result<DigestEvent, ProdromeError> {
        let said = match fields.name() {
            "Summarized" => Said::Summarized {
                text: fields.get("text")?,
                over: fields.get("over")?,
                model: fields.get("model")?,
                bytes: fields.get("bytes")?,
                taint: fields.get("taint")?,
            },
            "Unfit" => Said::Unfit {
                model: fields.get("model")?,
                limit: fields.get("limit")?,
                shortest: fields.get("shortest")?,
            },
            "Asked" => Said::Asked {
                sent: fields.get("sent")?,
            },
            "Retracted" => Said::Retracted,
            other => return Err(crate::literal::no_arm("a digest event", other)),
        };
        DigestEvent::checked(
            fields.get("node")?,
            fields.get("at")?,
            fields.get("actor")?,
            said,
        )
        .map_err(|failed| ProdromeError::invalid(failed.to_string()))
    }
}

/// Discrete, so not inflationary: a retraction may follow a summary and a summary a retraction.
impl Sole for DigestEvent {
    fn reading<'a>(frontier: &Frontier<'a, Self>) -> Vec<&'a Stamp<Self>> {
        frontier.candidates()
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::{canonical, parse_event};
    use prodrome_core::schema::Schema as _;
    use proptest::prelude::*;

    use super::*;

    fn at() -> Datetime {
        Datetime::new(2026, 10, 6, 9, 0, 0, 0).expect("an instant")
    }

    fn name(n: u8) -> Hash {
        Hash::new(format!("{n:064x}")).expect("a name")
    }

    fn model() -> ModelName {
        ModelName::parse("a-model").expect("a model")
    }

    fn attempt(over: Vec<Hash>, taint: Taint) -> Attempt {
        Attempt {
            over,
            model: model(),
            taint,
        }
    }

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        crate::literal::assert_own(SIGNATURES);
    }

    proptest! {
        /// Every event prints and parses back as itself, and its print never holds its words.
        #[test]
        fn every_event_prints_and_parses_back(
            words in "[A-Z]{3}[A-Z ]{8,60}[A-Z]", salt in proptest::array::uniform16(any::<u8>()),
            node in any::<u8>(), over in proptest::collection::vec(any::<u8>(), 1..16),
            tainted in any::<bool>(), sent in any::<u64>(), shortest in any::<u32>(),
        ) {
            let text = Text::new(Line::new(&words).expect("a line"), salt);
            let ada = Actor::new("ada").expect("an actor");
            let over = over.into_iter().map(name).collect();
            let taint = if tainted { Taint::Tainted } else { Taint::Clean };
            for event in [
                DigestEvent::summarized(name(node), at(), ada.clone(), &text, attempt(over, taint))
                    .expect("an event"),
                DigestEvent::unfit(name(node), at(), ada.clone(), model(), shortest),
                DigestEvent::asked(name(node), at(), ada.clone(), sent),
                DigestEvent::of(name(node), at(), ada, Said::Retracted),
            ] {
                let print = canonical(&event);
                prop_assert!(!print.contains(words.trim()), "{print}");
                prop_assert_eq!(parse_event::<DigestEvent>(&DigestVocabulary::default(), &print), Ok(event));
            }
        }
    }

    /// Every count prints as itself and reads back, at the widest it holds.
    #[test]
    fn every_count_round_trips_at_its_widest() {
        let ada = Actor::new("ada").expect("an actor");
        let unfit = DigestEvent::of(
            name(1),
            at(),
            ada.clone(),
            Said::Unfit {
                model: model(),
                limit: u32::MAX,
                shortest: u32::MAX,
            },
        );
        for event in [DigestEvent::asked(name(1), at(), ada, u64::MAX), unfit] {
            let print = canonical(&event);
            assert_eq!(
                parse_event::<DigestEvent>(&DigestVocabulary::default(), &print),
                Ok(event),
                "{print}"
            );
        }
    }

    #[test]
    fn a_summary_is_written_from_something_and_fits_a_node() {
        let text = Text::new(Line::new("a line").expect("a line"), [0; 16]);
        let ada = Actor::new("ada").expect("an actor");
        assert_eq!(
            DigestEvent::summarized(
                name(1),
                at(),
                ada.clone(),
                &text,
                attempt(Vec::new(), Taint::Clean)
            ),
            Err(NotADigestEvent::FromNothing)
        );
        let event = DigestEvent::summarized(
            name(1),
            at(),
            ada,
            &text,
            attempt(vec![name(2)], Taint::Clean),
        )
        .expect("ok");
        let long = canonical(&event).replace("bytes=6", &format!("bytes={}", NODE_BYTES + 1));
        assert!(parse_event::<DigestEvent>(&DigestVocabulary::default(), &long).is_err());
    }

    /// A call is counted and stands as no node's reading: it writes no register.
    #[test]
    fn a_call_writes_no_register() {
        let ada = Actor::new("ada").expect("an actor");
        assert_eq!(
            DigestEvent::asked(name(1), at(), ada.clone(), 9)
                .writes()
                .count(),
            0
        );
        assert_eq!(
            DigestEvent::asked(name(1), at(), ada.clone(), 9).sent(),
            Some(9)
        );
        assert_eq!(
            DigestEvent::unfit(name(1), at(), ada, model(), 9)
                .writes()
                .count(),
            1
        );
    }
}
