use std::sync::Arc;

use prodrome_core::event::Actor;
use prodrome_core::sign::{PublicKey, Registrar, Signed};

use super::{Keeper, Keeping, Trust, Unproven, Unread};

/// The trust of every chain that keeps keys, each read under the deployment's root.
#[derive(Debug, Clone)]
pub struct Trusting(Keeping<Arc<Trust>>);

impl Trusting {
    pub fn held(chains: &Keeping<Keeper<'_>>, root: &PublicKey) -> Result<Self, Unread> {
        let roots = Registrar::Roots([root.clone()].into_iter().collect());
        Ok(Self(match chains {
            Keeping::Keyless => Keeping::Keyless,
            Keeping::Keyed(keyed) => Keeping::Keyed(keyed.try_map(|chain| {
                Trust::held(chain.store, &roots).map_err(|error| Unread {
                    chain: chain.name.clone(),
                    error,
                })
            })?),
        }))
    }

    /// `signed` verifies and every keyed chain trusts its key for `actor`; a revocation on any
    /// keyed chain counts on all, and with no keyed chain no key is trusted.
    pub fn admits(&self, actor: &Actor, signed: &Signed) -> Result<(), Unproven> {
        match &self.0 {
            Keeping::Keyless => Err(Unproven::Untrusted {
                key: signed.key.clone(),
            }),
            Keeping::Keyed(keyed) => {
                if let Some(revoked) = self.revoked(actor, signed) {
                    return Err(revoked);
                }
                keyed
                    .iter()
                    .map(|trust| trust.admits(trust.home(), actor, signed))
                    .fold(Ok(()), Result::and)
            }
        }
    }

    /// [`Unproven::Revoked`] where any keyed chain revoked `signed`'s key for `actor`.
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
