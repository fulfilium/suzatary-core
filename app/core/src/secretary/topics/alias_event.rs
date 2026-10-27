//! The alias store's schema. A rename writes aliases and no membership: memberships stay under
//! the names they were written with, and the reading follows each name's alias.

use prodrome_core::event::Actor;
use prodrome_core::fold::{Discrete, RegisterType};
use prodrome_core::literal::{Datetime, ProdromeError, Value};

use crate::secretary::topics::topic::Topic;
use prodrome_core::fold::sole::{One, Only, Sole};
use prodrome_core::literal::codec::{Constructor, Fields, Literal as _};
use prodrome_core::schema::headed::{Body, Constructors, Event};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Alias {
    Itself,
    /// Never the topic itself: [`aliased`] refuses that.
    As(Topic),
}

/// A topic's alias event.
pub type AliasEvent = Event<Alias>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} cannot be an alias of itself; it is a name of its own")]
pub struct OfItself(pub Topic);

/// `topic` made an alias of `to` by `actor`.
///
/// # Errors
/// [`OfItself`] where `to` is `topic`, so "its own name" has one spelling, [`unaliased`].
pub fn aliased(
    topic: Topic,
    to: Topic,
    at: Datetime,
    actor: Actor,
) -> Result<AliasEvent, OfItself> {
    Event::new(topic, at, actor, Alias::As(to))
}

/// `topic` made a name of its own again by `actor`.
///
/// # Errors
/// None in practice: the schema refuses only an alias of itself. The type is the schema's one
/// door, [`Event::new`].
pub fn unaliased(topic: Topic, at: Datetime, actor: Actor) -> Result<AliasEvent, OfItself> {
    Event::new(topic, at, actor, Alias::Itself)
}

const ALIASED: Constructor = ("Aliased", &["topic", "at", "actor", "to"]);
const UNALIASED: Constructor = ("Unaliased", &["topic", "at", "actor"]);
const SIGNATURES: &[Constructor] = &[ALIASED, UNALIASED];

/// The alias store's schema.
pub type AliasVocabulary = Constructors<Alias>;

impl Body for Alias {
    type Key = Topic;
    type Register = Only;
    type Registers<'a> = One<'a, AliasType>;
    type Refusal = OfItself;

    const EVENTS: &'static [Constructor] = SIGNATURES;

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }

    fn admits(topic: &Topic, alias: &Alias) -> Result<(), OfItself> {
        match alias {
            Alias::As(to) if to == topic => Err(OfItself(topic.clone())),
            Alias::As(_) | Alias::Itself => Ok(()),
        }
    }

    fn fields(&self) -> (Constructor, Vec<Value>) {
        match self {
            Alias::As(to) => (ALIASED, vec![to.to_value()]),
            Alias::Itself => (UNALIASED, Vec::new()),
        }
    }

    fn parse(fields: &Fields<'_>) -> Result<AliasEvent, ProdromeError> {
        let (topic, at, actor) = (
            fields.get("topic")?,
            fields.get("at")?,
            fields.get("actor")?,
        );
        match fields.name() {
            "Aliased" => aliased(topic, fields.get("to")?, at, actor)
                .map_err(|failed| ProdromeError::invalid(failed.to_string())),
            "Unaliased" => unaliased(topic, at, actor)
                .map_err(|failed| ProdromeError::invalid(failed.to_string())),
            other => Err(prodrome_core::literal::codec::no_arm(
                "an alias event",
                other,
            )),
        }
    }
}

/// The register's type: an [`Alias`], discrete, so two different aliases written concurrently
/// for one topic are a conflict, which the reading shows and the next rename settles.
#[derive(Debug)]
pub struct AliasType;

impl RegisterType<AliasEvent> for AliasType {
    type Value<'e> = Discrete<&'e Alias>;

    fn value(event: &AliasEvent) -> Option<Discrete<&Alias>> {
        Some(Discrete(event.body()))
    }
}

impl Sole for AliasType {
    type Schema = AliasEvent;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        crate::own::assert_own(SIGNATURES);
    }

    #[test]
    fn a_topic_is_no_alias_of_itself() {
        let at = Datetime::new(2026, 10, 21, 9, 0, 0, 0).expect("an instant");
        let ada = Actor::new("ada").expect("an actor");
        let travel = Topic::new("travel").expect("a topic");
        assert_eq!(
            aliased(travel.clone(), travel.clone(), at, ada.clone()),
            Err(OfItself(travel.clone()))
        );
        let itself = prodrome_core::literal::codec::record(
            ALIASED,
            vec![
                travel.to_value(),
                at.to_value(),
                ada.to_value(),
                travel.to_value(),
            ],
        );
        assert!(
            <AliasEvent as prodrome_core::schema::Schema>::from_value(
                &AliasVocabulary::default(),
                &itself
            )
            .is_err()
        );
    }
}
