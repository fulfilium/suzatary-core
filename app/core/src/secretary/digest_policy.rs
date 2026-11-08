//! Whose word the digest believes about standing: a retraction binds only from one of the
//! deployment's owners, the actors the gate maps a login to (every one who can mint a
//! [`crate::signatures::Tap`]). Anyone else's, the secretary's, the triage agent's, a
//! plugin's or a compactor's, is stored and claims, and moves nothing. A summary binds whoever
//! wrote it, since writing summaries is what a compactor is for.
//!
//! An allowlist and not the reference policy's blocklist: a writer added tomorrow is not an
//! owner until the instance file says so.

use std::collections::BTreeSet;

use prodrome_core::event::{Actor, Hash};
use prodrome_core::policy::{Policy, Standing};
use prodrome_core::schema::Schema as _;

use crate::secretary::digest_event::DigestEvent;

/// The owners, by actor.
#[derive(Debug, Clone, Copy)]
pub struct Owners<'a>(pub &'a BTreeSet<Actor>);

/// An event about standing written as someone no owner.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{actor} is not one of the deployment's writers, so it cannot retract")]
pub struct NotAnOwner {
    /// Who wrote it.
    pub actor: Actor,
}

impl Owners<'_> {
    /// The store's door: an event about standing is taken only from an owner, so a forged one
    /// is never stored or pushed; the reading's policy is the same rule, for what arrived before.
    ///
    /// # Errors
    /// [`NotAnOwner`] for a retraction written as anyone else.
    pub fn admits(self, event: &DigestEvent) -> Result<(), NotAnOwner> {
        if self.binds(event) {
            Ok(())
        } else {
            Err(NotAnOwner {
                actor: event.actor().clone(),
            })
        }
    }

    fn binds(self, event: &DigestEvent) -> bool {
        !event.asks() || self.0.contains(event.actor())
    }
}

impl Policy<DigestEvent> for Owners<'_> {
    fn standing(&self, _object: &Hash, event: &DigestEvent) -> Standing {
        if self.binds(event) {
            Standing::Binds
        } else {
            Standing::Claims
        }
    }
}
