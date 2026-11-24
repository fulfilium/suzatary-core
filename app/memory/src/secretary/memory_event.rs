//! The memory's schema: each event is about one [`FactId`] and names its words only by
//! [`TextName`].
//!
//! The life register is inflationary, so the store refuses a `Remembered` on a fact already
//! forgotten.
//! A consolidation writes only the new fact's register; [`crate::secretary::two_phase`] reads that
//! it replaces its sources.
//! The words are never an object: they live in the memory's text store, which forgetting deletes
//! from.

use std::collections::BTreeSet;

use prodrome_data::event::Actor;
use prodrome_data::fold::{Climb, Discrete, RegisterType};
use prodrome_data::literal::{Datetime, ProdromeError, Value};

use prodrome_data::fold::sole::{One, Only, Sole};
use prodrome_data::literal::codec::{Constructor, Fields, Literal as _};
use prodrome_data::schema::headed::{Body, Constructors, Event};

use crate::secretary::fact_life::Life;
use crate::secretary::memory::{FactId, Kind, Origin, TAPPED, TURN};
use crate::secretary::salted_text::TextName;

pub type MemoryEvent = Event<Said>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Remembered {
        text: TextName,
        kind: Kind,
        origin: Origin,
    },
    /// It replaces the facts `from` with one fact of its own words, and its origins are theirs.
    Consolidated {
        /// At least one, each numbered below it.
        from: BTreeSet<FactId>,
        text: TextName,
        kind: Kind,
        /// The round the owner opened that wrote its words.
        round: Origin,
    },
    /// It was forgotten; `version` is the memory's version this made.
    Forgotten {
        /// One more than every id and version before it.
        version: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAMemoryEvent {
    #[error("fact {fact} cannot be forgotten at version {version}, at or below its own number")]
    Before { fact: FactId, version: u64 },
    #[error("fact {0} consolidates no fact")]
    FromNothing(FactId),
    #[error("fact {fact} cannot replace fact {replaced}, numbered at or above it")]
    Later {
        fact: FactId,
        /// The source numbered at or above it.
        replaced: FactId,
    },
}

/// # Errors
/// None in practice: the schema refuses only a forgetting.
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

pub fn forgotten(
    fact: FactId,
    at: Datetime,
    actor: Actor,
    version: u64,
) -> Result<MemoryEvent, NotAMemoryEvent> {
    Event::new(fact, at, actor, Said::Forgotten { version })
}

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

const REMEMBERED: Constructor = (
    "Remembered",
    &["fact", "at", "actor", "text", "kind", "said"],
);
const FORGOTTEN: Constructor = ("Forgotten", &["fact", "at", "actor", "version"]);
const CONSOLIDATED: Constructor = (
    "Consolidated",
    &["fact", "at", "actor", "from", "text", "kind", "round"],
);
/// The memory's whitelist: its three events and an origin's two arms, because no event nests a
/// term.
pub type MemoryVocabulary = Constructors<Said>;

impl Body for Said {
    const NAME: &'static str = "Said";
    type Key = FactId;
    type Register = Only;
    type Registers<'a> = One<'a, LifeRegister>;
    type Refusal = NotAMemoryEvent;

    const EVENTS: &'static [Constructor] = &[REMEMBERED, FORGOTTEN, CONSOLIDATED];
    const NESTED: &'static [Constructor] = &[TURN, TAPPED];

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }

    /// A forgetting and a consolidation come after the facts they name, so no fact rests on itself.
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
                return Err(prodrome_data::literal::codec::no_arm(Self::NAME, other));
            }
        }
        .map_err(|failed| ProdromeError::invalid(failed.to_string()))
    }
}

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

impl Sole for LifeRegister {
    type Schema = MemoryEvent;
    type Held<'a> = Climb<'a, MemoryEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        suzatary_base::own::assert_own(&[REMEMBERED, FORGOTTEN, CONSOLIDATED, TURN, TAPPED]);
    }

    /// Every event reads back as itself at its widest counts.
    #[test]
    fn every_event_round_trips_at_its_widest() {
        use prodrome_data::event::{canonical, parse_event};
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
                    conversation: suzatary_base::conversation_id::ConversationId::new("default")
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
                    conversation: suzatary_base::conversation_id::ConversationId::new("default")
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
                    conversation: suzatary_base::conversation_id::ConversationId::new("default")
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
