//! The memory's schema: a fact remembered, naming its words by their [`TextName`], and a fact
//! forgotten, naming only its id, each about one [`FactId`].
//!
//! Every event writes the fact's one register, its life ([`LifeRegister`]), declared
//! inflationary: the store refuses a
//! `Remembered` on a fact already forgotten, so once forgotten, always forgotten. The words are
//! never an object: they live in the memory's text store, which forgetting deletes from.

use prodrome_core::event::Actor;
use prodrome_core::fold::{Discrete, Frontier, RegisterType};
use prodrome_core::literal::{Datetime, ProdromeError, Value};

use prodrome_core::fold::sole::{One, Only, Sole};
use prodrome_core::literal::codec::{Constructor, Fields, Literal as _};
use prodrome_core::schema::headed::{Body, Constructors, Event};

use crate::secretary::fact_life::Life;
use crate::secretary::memory::{FactId, Kind, Origin, TAPPED, TURN};
use crate::secretary::salted_text::TextName;

/// One event about one fact: which, when, who wrote it, and what became of it.
pub type MemoryEvent = Event<Said>;

/// What an event says of its fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// It was remembered: its words by name, about whom, and who said it.
    Remembered {
        /// The name of its words in the text store.
        text: TextName,
        /// About whom or what.
        kind: Kind,
        /// Who said it.
        origin: Origin,
    },
    /// It was forgotten; `version` is the memory's version this made.
    Forgotten {
        /// One more than every id and version before it.
        version: u64,
    },
}

/// An event that cannot be.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAMemoryEvent {
    /// A forgetting numbered at or below the fact it forgets, which was remembered first.
    #[error("fact {fact} cannot be forgotten at version {version}, at or below its own number")]
    Before {
        /// The fact.
        fact: FactId,
        /// The version.
        version: u64,
    },
}

/// Fact `fact` remembered by `actor`: its words by `text`, its `kind` and who said it.
///
/// # Errors
/// None in practice: the schema refuses only a forgetting. The type is the schema's one door,
/// [`Event::new`].
pub fn remembered(
    fact: FactId,
    at: Datetime,
    actor: Actor,
    text: TextName,
    kind: Kind,
    origin: Origin,
) -> Result<MemoryEvent, NotAMemoryEvent> {
    Event::new(fact, at, actor, Said::Remembered { text, kind, origin })
}

/// Fact `fact` forgotten by `actor`, making memory version `version`.
///
/// # Errors
/// [`NotAMemoryEvent::Before`] for a version at or below the fact's own number.
pub fn forgotten(
    fact: FactId,
    at: Datetime,
    actor: Actor,
    version: u64,
) -> Result<MemoryEvent, NotAMemoryEvent> {
    Event::new(fact, at, actor, Said::Forgotten { version })
}

/// `Remembered(fact, at, actor, text, kind, said)`.
const REMEMBERED: Constructor = (
    "Remembered",
    &["fact", "at", "actor", "text", "kind", "said"],
);
/// `Forgotten(fact, at, actor, version)`.
const FORGOTTEN: Constructor = ("Forgotten", &["fact", "at", "actor", "version"]);
/// The memory's whitelist: its two events and an origin's two arms, nothing else, since no
/// event nests a term.
pub type MemoryVocabulary = Constructors<Said>;

impl Body for Said {
    type Key = FactId;
    type Register = Only;
    type Registers<'a> = One<'a, LifeRegister>;
    type Refusal = NotAMemoryEvent;

    const EVENTS: &'static [Constructor] = &[REMEMBERED, FORGOTTEN];
    const NESTED: &'static [Constructor] = &[TURN, TAPPED];

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }

    /// A forgetting comes after its fact: its version is above the fact's own number.
    fn admits(fact: &FactId, said: &Said) -> Result<(), NotAMemoryEvent> {
        match *said {
            Said::Forgotten { version } if version <= fact.0 => Err(NotAMemoryEvent::Before {
                fact: *fact,
                version,
            }),
            Said::Remembered { .. } | Said::Forgotten { .. } => Ok(()),
        }
    }

    fn fields(&self) -> (Constructor, Vec<Value>) {
        match self {
            Said::Remembered { text, kind, origin } => (
                REMEMBERED,
                vec![text.to_value(), kind.to_value(), origin.to_value()],
            ),
            Said::Forgotten { version } => (FORGOTTEN, vec![version.to_value()]),
        }
    }

    fn parse(fields: &Fields<'_>) -> Result<MemoryEvent, ProdromeError> {
        let (fact, at, actor) = (fields.get("fact")?, fields.get("at")?, fields.get("actor")?);
        match fields.name() {
            "Remembered" => remembered(
                fact,
                at,
                actor,
                fields.get("text")?,
                fields.get("kind")?,
                fields.get("said")?,
            ),
            "Forgotten" => forgotten(fact, at, actor, fields.get("version")?),
            other => {
                return Err(prodrome_core::literal::codec::no_arm(
                    "a memory event",
                    other,
                ));
            }
        }
        .map_err(|failed| ProdromeError::invalid(failed.to_string()))
    }
}

/// The register's type: a [`Life`].
#[derive(Debug)]
pub struct LifeRegister;

impl RegisterType<MemoryEvent> for LifeRegister {
    type Value<'e> = Life<'e>;

    fn value(event: &MemoryEvent) -> Option<Life<'_>> {
        Some(match event.body() {
            Said::Remembered { .. } => Life::Remembered(Discrete(event)),
            Said::Forgotten { .. } => Life::Forgotten,
        })
    }
}

/// Inflationary: the store refuses a write below the life it supersedes.
impl Sole for LifeRegister {
    type Schema = MemoryEvent;

    fn grows(
        frontier: &Frontier<'_, MemoryEvent>,
        event: &MemoryEvent,
    ) -> Result<(), ProdromeError> {
        frontier.grows::<Self>(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        crate::own::assert_own(&[REMEMBERED, FORGOTTEN, TURN, TAPPED]);
    }

    /// Every count prints as itself and reads back, at the widest it holds.
    #[test]
    fn every_event_round_trips_at_its_widest() {
        use prodrome_core::event::{canonical, parse_event};
        let at = Datetime::new(2026, 10, 11, 9, 0, 0, 0).expect("an instant");
        let ada = Actor::new("ada").expect("an actor");
        let fact = FactId(u64::MAX - 1);
        for event in [
            remembered(
                fact,
                at,
                ada.clone(),
                TextName::new("a".repeat(64)).expect("a name"),
                Kind::Owner,
                Origin::Turn {
                    conversation: crate::secretary::store::ConversationId::new("default")
                        .expect("an id"),
                    turn: usize::MAX,
                },
            )
            .expect("a remembering"),
            forgotten(fact, at, ada, u64::MAX).expect("after its fact"),
        ] {
            let print = canonical(&event);
            assert_eq!(
                parse_event(&MemoryVocabulary::default(), &print),
                Ok(event),
                "{print}"
            );
        }
    }

    #[test]
    fn a_forgetting_comes_after_its_fact() {
        let at = Datetime::new(2026, 10, 2, 9, 0, 0, 0).expect("an instant");
        let ada = Actor::new("ada").expect("an actor");
        assert!(forgotten(FactId(3), at, ada.clone(), 3).is_err());
        assert!(forgotten(FactId(3), at, ada, 4).is_ok());
    }
}
