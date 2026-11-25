//! Whose word the digest believes about standing: a retraction binds only from one of the
//! deployment's owners. An allowlist, not a blocklist: a writer added tomorrow is not an owner
//! until the instance file says so.

use std::collections::BTreeSet;

use prodrome_dag::policy::Policy;
use prodrome_data::event::{Actor, Hash};
use prodrome_data::policy::Standing;
use prodrome_data::schema::Schema as _;

use crate::secretary::digest_event::DigestEvent;

#[derive(Debug, Clone, Copy)]
pub struct Owners<'a>(pub &'a BTreeSet<Actor>);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{actor} is not one of the deployment's writers, so it cannot retract")]
pub struct NotAnOwner {
    pub actor: Actor,
}

impl Owners<'_> {
    /// The store's door: an event about standing is taken only from an owner, so a forged one
    /// is never stored or pushed; the reading's policy is the same rule, for what arrived before.
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
