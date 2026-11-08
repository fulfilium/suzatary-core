use prodrome_core::event::Actor;
use prodrome_core::policy::Untrusted;

use crate::writer::Writer;

/// WHOSE KEY SIGNS AN ACTOR'S WRITES, and so the one key of the box's two registered for it:
/// the agent's for the actor the agent writes as ([`Writer::Triage`]) and for any actor
/// a chain does not trust, the box's for every other.
///
/// A fact about who writes, never about a chain's fold: the agent signs with its own key whatever
/// a chain's `untrusted` names, so a chain that trusts nobody still registers the agent's key, and
/// never the box's, for the agent's actor. Registering, signing and revoking a misplaced key all
/// read this one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Custody {
    /// The box's key, the root.
    Box,
    /// The agent's key.
    Agent,
}

impl Custody {
    /// Who holds the key `actor`'s writes are signed with, on a chain that does not trust
    /// `untrusted`.
    #[must_use]
    pub fn of(actor: &Actor, untrusted: &Untrusted) -> Custody {
        if actor.as_str() == Writer::Triage.actor() || untrusted.actors().contains(actor) {
            Custody::Agent
        } else {
            Custody::Box
        }
    }

    /// The other holder: whose key must never speak for an actor of this custody.
    #[must_use]
    pub fn other(self) -> Custody {
        match self {
            Custody::Box => Custody::Agent,
            Custody::Agent => Custody::Box,
        }
    }
}
