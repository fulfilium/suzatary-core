use prodrome_core::event::Actor;

use super::{BoxKeys, Custody};
use prodrome_core::policy::Untrusted;
use suzatary_store::store::Signer;

/// What the box signs with, as the deployment declares and secretspec answers: resolved once
/// per process, and never a reason not to start, since a box whose keys are locked away must
/// still read.
#[derive(Debug, Clone)]
pub enum Signers {
    /// The keys not at hand; the sentence says which and what to run.
    Missing(String),
    /// Both keys, checked against the root.
    Held(BoxKeys),
}

impl Signers {
    /// The key `actor`'s writes are signed with, as [`Custody`] says, which is the key
    /// adoption registers for it. So no event is signed by a key its actor does not own.
    #[must_use]
    pub fn for_actor(&self, actor: &Actor, untrusted: &Untrusted) -> Signer {
        match self {
            Signers::Missing(why) => Signer::Refusing(why.clone()),
            Signers::Held(keys) => {
                Signer::Key(keys.of_custody(Custody::of(actor, untrusted)).clone())
            }
        }
    }

    /// The keys, when held.
    #[must_use]
    pub fn held(&self) -> Option<&BoxKeys> {
        match self {
            Signers::Held(keys) => Some(keys),
            Signers::Missing(_) => None,
        }
    }
}
