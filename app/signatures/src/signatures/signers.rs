use prodrome_core::event::Actor;

use super::{BoxKeys, Custody};
use prodrome_core::policy::Untrusted;
use suzatary_store::store::Signer;

/// What the box signs with, resolved once per process; missing keys never stop the box, which
/// must still read.
#[derive(Debug, Clone)]
pub enum Signers {
    /// The sentence says which keys are missing and what to run.
    Missing(String),
    /// Both keys, checked against the root.
    Held(BoxKeys),
}

impl Signers {
    /// The key [`Custody`] gives `actor`, the one adoption registers for it.
    #[must_use]
    pub fn for_actor(&self, actor: &Actor, untrusted: &Untrusted) -> Signer {
        match self {
            Signers::Missing(why) => Signer::Refusing(why.clone()),
            Signers::Held(keys) => {
                Signer::Key(keys.of_custody(Custody::of(actor, untrusted)).clone())
            }
        }
    }

    #[must_use]
    pub fn held(&self) -> Option<&BoxKeys> {
        match self {
            Signers::Held(keys) => Some(keys),
            Signers::Missing(_) => None,
        }
    }
}
