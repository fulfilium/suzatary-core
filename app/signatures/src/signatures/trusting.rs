use std::sync::Arc;

use prodrome_core::event::Actor;
use prodrome_core::sign::{PublicKey, Registrar, Signed};

use super::{Trust, Unproven};
use suzatary_record::record::TodoEvent;
use suzatary_store::store::{Prodrome, StoreError};

/// The trust of every chain the box holds, each read under the deployment's root.
#[derive(Debug, Clone)]
pub struct Trusting(Vec<Arc<Trust>>);

impl Trusting {
    /// The trust of `chains`, each read under `root`.
    ///
    /// # Errors
    /// A chain unreadable.
    pub fn held(chains: &[&dyn Prodrome<TodoEvent>], root: &PublicKey) -> Result<Self, StoreError> {
        let roots = Registrar::Roots([root.clone()].into_iter().collect());
        chains
            .iter()
            .map(|chain| Trust::held(*chain, &roots))
            .collect::<Result<_, _>>()
            .map(Self)
    }

    /// Readings already made.
    #[must_use]
    pub fn of(trusts: Vec<Arc<Trust>>) -> Self {
        Self(trusts)
    }

    /// `signed` verifies, and every chain trusts its key for `actor`; a revocation on any chain
    /// counts on all, and no chain at all trusts nothing.
    ///
    /// # Errors
    /// [`Unproven::Revoked`] where any chain revoked the key, else the first chain's refusal.
    pub fn admits(&self, actor: &Actor, signed: &Signed) -> Result<(), Unproven> {
        if let Some(revoked) = self.revoked(actor, signed) {
            return Err(revoked);
        }
        let mut answers = self
            .0
            .iter()
            .map(|trust| trust.admits(trust.home(), actor, signed));
        match answers.next() {
            None => Err(Unproven::Untrusted {
                key: signed.key.clone(),
            }),
            Some(first) => answers.fold(first, Result::and),
        }
    }

    /// [`Unproven::Revoked`] where any chain revoked `signed`'s key for `actor`.
    #[must_use]
    pub fn revoked(&self, actor: &Actor, signed: &Signed) -> Option<Unproven> {
        self.0
            .iter()
            .find_map(|trust| match trust.admits(trust.home(), actor, signed) {
                Err(why @ Unproven::Revoked { .. }) => Some(why),
                Ok(())
                | Err(Unproven::Unsigned | Unproven::Forged | Unproven::Untrusted { .. }) => None,
            })
    }
}
