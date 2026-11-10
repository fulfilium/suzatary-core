//! Who set a topic on a target: the owner, whose topic is kept, or the secretary, whose topic is a
//! suggestion it may revise or drop.

use prodrome_core::event::Actor;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Ordered `Inferred < Owner`, so the join of a frontier's writers is the owner's where any is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum Provenance {
    /// The secretary wrote it.
    Inferred,
    /// Any other actor wrote it: every other writer of a topic store is a device of the owner.
    Owner,
}

impl Provenance {
    #[must_use]
    pub fn of(actor: &Actor) -> Self {
        if *actor == crate::secretary::transcript::writer() {
            Self::Inferred
        } else {
            Self::Owner
        }
    }
}

/// An actor who is not the secretary: the only writer the owner's planners take, so the
/// secretary's actor cannot reach them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner(Actor);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} is the secretary, which suggests topics and never sets the owner's")]
pub struct NotTheOwner(Actor);

impl Owner {
    /// # Errors
    /// [`NotTheOwner`] for the secretary's actor.
    pub fn new(actor: Actor) -> Result<Self, NotTheOwner> {
        match Provenance::of(&actor) {
            Provenance::Owner => Ok(Self(actor)),
            Provenance::Inferred => Err(NotTheOwner(actor)),
        }
    }

    #[must_use]
    pub fn actor(&self) -> &Actor {
        &self.0
    }
}
