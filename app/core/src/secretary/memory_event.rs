//! The memory's schema: a fact remembered, naming its words by their [`TextName`]; a fact
//! consolidated from earlier facts, naming its words and the facts it replaces; and a fact
//! forgotten, naming only its id. Each event is about one [`FactId`].
//!
//! Every event writes the fact's one register, its life ([`LifeRegister`]), declared
//! inflationary: the store refuses a
//! `Remembered` on a fact already forgotten, so once forgotten, always forgotten. A
//! consolidation writes only the new fact's register; that it replaces its sources is the
//! reading's ([`crate::secretary::two_phase`]). The words are never an object: they live in the
//! memory's text store, which forgetting and consolidating delete from.

use std::collections::BTreeSet;

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
    /// It replaces the facts `from` with one fact of its own words: what they said together,
    /// on their authority, so its origins are theirs.
    Consolidated {
        /// The facts it replaces: at least one, each numbered below it.
        from: BTreeSet<FactId>,
        /// The name of its words in the text store.
        text: TextName,
        /// About whom or what.
        kind: Kind,
        /// The round the owner opened that wrote it, where its words were written.
        round: Origin,
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
    /// A consolidation that replaces nothing.
    #[error("fact {0} consolidates no fact")]
    FromNothing(FactId),
    /// A consolidation naming a source numbered at or above itself, which cannot be earlier.
    #[error("fact {fact} cannot replace fact {replaced}, numbered at or above it")]
    Later {
        /// The fact.
        fact: FactId,
        /// The source numbered at or above it.
        replaced: FactId,
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

/// Fact `fact` written by `actor` in `round` from the facts `from`, its words by `text`.
///
/// # Errors
/// [`NotAMemoryEvent::FromNothing`] for no source, and [`NotAMemoryEvent::Later`] for a source
/// numbered at or above `fact`.
pub fn consolidated(
    fact: FactId,
    at: Datetime,
    actor: Actor,
    from: BTreeSet<FactId>,
    text: TextName,
    kind: Kind,
    round: Origin,
) -> Result<MemoryEvent, NotAMemoryEvent> {
    Event::new(
        fact,
        at,
        actor,
        Said::Consolidated {
            from,
            text,
            kind,
            round,
        },
    )
}

/// `Remembered(fact, at, actor, text, kind, said)`.
const REMEMBERED: Constructor = (
    "Remembered",
    &["fact", "at", "actor", "text", "kind", "said"],
);
/// `Forgotten(fact, at, actor, version)`.
const FORGOTTEN: Constructor = ("Forgotten", &["fact", "at", "actor", "version"]);
/// `Consolidated(fact, at, actor, from, text, kind, round)`.
const CONSOLIDATED: Constructor = (
    "Consolidated",
    &["fact", "at", "actor", "from", "text", "kind", "round"],
);
/// The memory's whitelist: its three events and an origin's two arms, nothing else, since no
/// event nests a term.
pub type MemoryVocabulary = Constructors<Said>;

impl Body for Said {
    type Key = FactId;
    type Register = Only;
    type Registers<'a> = One<'a, LifeRegister>;
    type Refusal = NotAMemoryEvent;

    const EVENTS: &'static [Constructor] = &[REMEMBERED, FORGOTTEN, CONSOLIDATED];
    const NESTED: &'static [Constructor] = &[TURN, TAPPED];

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }

    /// A forgetting comes after its fact: its version is above the fact's own number. A
    /// consolidation comes after every fact it replaces, so no fact rests on itself.
    fn admits(fact: &FactId, said: &Said) -> Result<(), NotAMemoryEvent> {
        match said {
            Said::Forgotten { version } if *version <= fact.0 => Err(NotAMemoryEvent::Before {
                fact: *fact,
                version: *version,
            }),
            Said::Consolidated { from, .. } => match from.last() {
                None => Err(NotAMemoryEvent::FromNothing(*fact)),
                Some(replaced) if replaced >= fact => Err(NotAMemoryEvent::Later {
                    fact: *fact,
                    replaced: *replaced,
                }),
                Some(_) => Ok(()),
            },
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
            Said::Consolidated {
                from,
                text,
                kind,
                round,
            } => (
                CONSOLIDATED,
                vec![
                    from.to_value(),
                    text.to_value(),
                    kind.to_value(),
                    round.to_value(),
                ],
            ),
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
            "Consolidated" => consolidated(
                fact,
                at,
                actor,
                fields.get("from")?,
                fields.get("text")?,
                fields.get("kind")?,
                fields.get("round")?,
            ),
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
            Said::Remembered { .. } | Said::Consolidated { .. } => {
                Life::Remembered(Discrete(event))
            }
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
        crate::own::assert_own(&[REMEMBERED, FORGOTTEN, CONSOLIDATED, TURN, TAPPED]);
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
            forgotten(fact, at, ada.clone(), u64::MAX).expect("after its fact"),
            consolidated(
                FactId(u64::MAX),
                at,
                ada,
                [FactId(0), fact].into(),
                TextName::new("b".repeat(64)).expect("a name"),
                Kind::Standing,
                Origin::Tapped {
                    conversation: crate::secretary::store::ConversationId::new("default")
                        .expect("an id"),
                    proposal: usize::MAX,
                },
            )
            .expect("after its sources"),
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
    fn a_forgetting_and_a_consolidation_come_after_their_facts() {
        let at = Datetime::new(2026, 10, 2, 9, 0, 0, 0).expect("an instant");
        let ada = Actor::new("ada").expect("an actor");
        assert!(forgotten(FactId(3), at, ada.clone(), 3).is_err());
        assert!(forgotten(FactId(3), at, ada.clone(), 4).is_ok());
        let consolidating = |from: &[u64]| {
            consolidated(
                FactId(3),
                at,
                ada.clone(),
                from.iter().copied().map(FactId).collect(),
                TextName::new("a".repeat(64)).expect("a name"),
                Kind::Owner,
                Origin::Turn {
                    conversation: crate::secretary::store::ConversationId::new("default")
                        .expect("an id"),
                    turn: 0,
                },
            )
        };
        assert_eq!(
            consolidating(&[]),
            Err(NotAMemoryEvent::FromNothing(FactId(3)))
        );
        assert_eq!(
            consolidating(&[1, 3]),
            Err(NotAMemoryEvent::Later {
                fact: FactId(3),
                replaced: FactId(3)
            })
        );
        assert!(consolidating(&[1, 2]).is_ok());
    }
}
