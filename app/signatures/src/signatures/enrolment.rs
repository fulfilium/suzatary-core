use prodrome_core::dag::Dag;
use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::sign::{KeyAdded, PublicKey, Registrar, Secret, Signature, Signed};
use serde::{Deserialize, Serialize};

use super::{KeyStanding, Keyed, Trust, Unproven};
use suzatary_record::record::TodoEvent;
use suzatary_store::store::{Prodrome, StoreError};

/// A device asking to be trusted, as its Inbox card holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Spelled", into = "Spelled")]
pub struct Enrolment {
    /// The actor of the login that asked, never a request field.
    pub actor: Actor,
    pub key: PublicKey,
}

#[derive(Serialize, Deserialize)]
struct Spelled {
    actor: String,
    key: String,
}

impl TryFrom<Spelled> for Enrolment {
    type Error = String;
    fn try_from(spelled: Spelled) -> Result<Self, String> {
        Ok(Enrolment {
            actor: Actor::new(spelled.actor).map_err(|e| e.to_string())?,
            key: PublicKey::new(spelled.key).map_err(|e| e.to_string())?,
        })
    }
}

impl From<Enrolment> for Spelled {
    fn from(enrolment: Enrolment) -> Self {
        Spelled {
            actor: enrolment.actor.as_str().to_owned(),
            key: enrolment.key.as_str().to_owned(),
        }
    }
}

impl Enrolment {
    /// The `KeyAdded` for `history`'s first prodrome, which a trusted device signs to vouch.
    #[must_use]
    pub fn added(&self, history: &Dag<TodoEvent>) -> Option<Envelope<TodoEvent>> {
        history.geneses().into_iter().next().map(|genesis| {
            Envelope::KeyAdded(KeyAdded {
                genesis,
                actor: self.actor.clone(),
                key: self.key.clone(),
            })
        })
    }
}

/// A trusted device's signature of the registration a device card asks for, stored beside it so
/// anyone holding the chain can check who vouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endorsement {
    pub key: PublicKey,
    pub signature: Signature,
}

impl Endorsement {
    #[must_use]
    pub fn of(&self, object: &Hash) -> Signed {
        Signed {
            object: object.clone(),
            key: self.key.clone(),
            signature: self.signature.clone(),
        }
    }
}

/// The `Signed` to store if `endorsement` vouches for `enrolment` from a key trusted for `tapper`
/// in every prodrome of `history`.
pub(crate) fn endorses(
    history: &Dag<TodoEvent>,
    root: &PublicKey,
    tapper: &Actor,
    enrolment: &Enrolment,
    endorsement: &Endorsement,
) -> Result<Signed, Unproven> {
    let added = enrolment.added(history).ok_or(Unproven::Unsigned)?;
    let signed = endorsement.of(&seal_hash(&added));
    if !signed.verifies() {
        return Err(Unproven::Forged);
    }
    let trust = Trust::of(
        history,
        &Registrar::Roots([root.clone()].into_iter().collect()),
    );
    for genesis in history.geneses() {
        match trust.standing(&history.key(&genesis), tapper, &signed.key) {
            KeyStanding::Trusted => {}
            KeyStanding::Revoked => {
                return Err(Unproven::Revoked {
                    key: signed.key.clone(),
                });
            }
            KeyStanding::Untrusted => {
                return Err(Unproven::Untrusted {
                    key: signed.key.clone(),
                });
            }
        }
    }
    Ok(signed)
}

/// The tap's door to keys; held by no record, and only the tap route builds one.
#[derive(Clone, Copy)]
pub struct Enrolling<'a> {
    /// The first is where an endorsement is read and kept.
    pub chains: &'a Keyed<&'a dyn Prodrome<TodoEvent>>,
    pub root: &'a Secret,
}

impl Enrolling<'_> {
    /// Whether `endorsement` is a trusted device of `tapper`'s vouching for `enrolment`, read
    /// against the first keyed chain; answers what it signed.
    pub fn endorsed(
        &self,
        tapper: &Actor,
        enrolment: &Enrolment,
        endorsement: &Endorsement,
    ) -> Result<Result<Signed, Unproven>, StoreError> {
        let history: Dag<TodoEvent> = self.chains.first().objects()?.into_iter().collect();
        Ok(endorses(
            &history,
            &self.root.public(),
            tapper,
            enrolment,
            endorsement,
        ))
    }

    /// Registers the key on every keyed chain and keeps the endorsement on the first.
    ///
    /// # Errors
    /// A store refusing; earlier chains keep the key, and a second tap writes the same objects.
    pub fn enrol(&self, enrolment: &Enrolment, endorsed: &Signed) -> Result<(), StoreError> {
        for chain in self.chains.iter() {
            super::trust(*chain, self.root, &enrolment.actor, &enrolment.key)?;
        }
        let object = Envelope::<TodoEvent>::Signed(endorsed.clone());
        self.chains
            .first()
            .accept(&[(seal_hash(&object), canonical_envelope(&object))])?;
        Ok(())
    }
}
