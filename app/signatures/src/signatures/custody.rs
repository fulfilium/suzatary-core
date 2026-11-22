use prodrome_core::event::Actor;
use prodrome_core::policy::Untrusted;

use suzatary_base::writer::Writer;

/// Whose key signs an actor's writes: the agent's for [`Writer::Triage`] and any untrusted actor,
/// the box's for every other.
///
/// The agent's actor gets the agent's key even on a chain that trusts nobody. Registering,
/// signing and revoking a misplaced key all read this one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Custody {
    /// The box's key, the root.
    Box,
    Agent,
}

impl Custody {
    #[must_use]
    pub fn of(actor: &Actor, untrusted: &Untrusted) -> Custody {
        if actor.as_str() == Writer::Triage.actor() || untrusted.actors().contains(actor) {
            Custody::Agent
        } else {
            Custody::Box
        }
    }

    /// Whose key must never speak for an actor of this custody.
    #[must_use]
    pub fn other(self) -> Custody {
        match self {
            Custody::Box => Custody::Agent,
            Custody::Agent => Custody::Box,
        }
    }
}
