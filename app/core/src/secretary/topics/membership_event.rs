//! The membership store's schema: `Added` and `Removed`, each writing its pair's one register.

use prodrome_core::event::Actor;
use prodrome_core::fold::{Product as _, RegisterType};
use prodrome_core::literal::{Datetime, ProdromeError, Value};

use crate::secretary::topics::membership::{Membership, Presence};
use crate::secretary::topics::target::Target;
use crate::secretary::topics::topic::Topic;
use prodrome_core::fold::sole::{One, Only, Sole};
use prodrome_core::literal::codec::{Constructor, Fields, Literal as _};
use prodrome_core::schema::headed::{Body, Constructors, Event, KeyFields};

/// A topic put on a target or taken off it.
pub type MembershipEvent = Event<Presence>;

/// `topic` put on `target` by `actor`.
#[must_use]
pub fn added(target: Target, topic: Topic, at: Datetime, actor: Actor) -> MembershipEvent {
    Event::of(Membership { target, topic }, at, actor, Presence::On)
}

/// `topic` taken off `target` by `actor`.
#[must_use]
pub fn removed(target: Target, topic: Topic, at: Datetime, actor: Actor) -> MembershipEvent {
    Event::of(Membership { target, topic }, at, actor, Presence::Off)
}

const ADDED: Constructor = ("Added", &["target", "topic", "at", "actor"]);
const REMOVED: Constructor = ("Removed", &["target", "topic", "at", "actor"]);
const SIGNATURES: &[Constructor] = &[ADDED, REMOVED];

/// The membership store's schema.
pub type MembershipVocabulary = Constructors<Presence>;

/// A membership's key is two fields, its target's and its topic's.
impl KeyFields for Membership {
    fn key_fields(&self) -> Vec<Value> {
        vec![self.target.to_value(), self.topic.to_value()]
    }
}

impl Body for Presence {
    type Key = Membership;
    type Register = Only;
    type Registers<'a> = One<'a, PresenceRegister>;
    type Refusal = std::convert::Infallible;

    const EVENTS: &'static [Constructor] = SIGNATURES;

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }

    fn fields(&self) -> (Constructor, Vec<Value>) {
        match self {
            Presence::On => (ADDED, Vec::new()),
            Presence::Off => (REMOVED, Vec::new()),
        }
    }

    fn parse(fields: &Fields<'_>) -> Result<MembershipEvent, ProdromeError> {
        let presence = match fields.name() {
            "Added" => Presence::On,
            "Removed" => Presence::Off,
            other => {
                return Err(prodrome_core::literal::codec::no_arm(
                    "a membership event",
                    other,
                ));
            }
        };
        let membership = Membership {
            target: fields.get("target")?,
            topic: fields.get("topic")?,
        };
        Ok(MembershipEvent::of(
            membership,
            fields.get("at")?,
            fields.get("actor")?,
            presence,
        ))
    }
}

/// The register's type: a [`Presence`]. It is not inflationary: a removal after an add goes
/// down, and only causal order decides that it does.
#[derive(Debug)]
pub struct PresenceRegister;

impl RegisterType<MembershipEvent> for PresenceRegister {
    type Value<'e> = Presence;

    fn value(event: &MembershipEvent) -> Option<Presence> {
        Some(*event.body())
    }
}

impl Sole for PresenceRegister {
    type Schema = MembershipEvent;
}

impl Presence {
    /// The presence `registers` read: `Off` where nothing wrote it.
    #[must_use]
    pub fn of(registers: &One<'_, PresenceRegister>) -> Presence {
        registers
            .reading(Only)
            .iter()
            .map(|stamp| *stamp.event.body())
            .max()
            .unwrap_or(Presence::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        crate::own::assert_own(SIGNATURES);
    }
}
