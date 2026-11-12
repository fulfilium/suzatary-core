use prodrome_core::dag::Dag;
use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::sign::{KeyAdded, PublicKey, Registrar, Secret, Signature, Signed};
use serde::{Deserialize, Serialize};

use super::{KeyStanding, Trust, Unproven};
use crate::store::{Prodrome, StoreError};
use suzatary_record::record::TodoEvent;

/// A device asking to be trusted: whose it is and its public key, as its Inbox card holds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Spelled", into = "Spelled")]
pub struct Enrolment {
    /// Whom the key would speak for: the actor of the login that asked, never a request field.
    pub actor: Actor,
    /// The device's public key.
    pub key: PublicKey,
}

/// An enrolment as text, its one serde shape.
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
    /// The `KeyAdded` this enrolment writes into `history`'s first prodrome: the object a
    /// trusted device signs to vouch for it, and the box registers. `None` for a history that
    /// holds no prodrome yet.
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

/// What a trusted device says when it taps a device card: its key's signature of the
/// registration the card asks for, as the first chain would hold it. Stored beside that
/// registration, so who vouched is a fact anyone holding the chain can check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endorsement {
    /// The tapping device's key.
    pub key: PublicKey,
    /// Its signature of the registration's name.
    pub signature: Signature,
}

impl Endorsement {
    /// The endorsement as the object it is: a `Signed` of `object`.
    #[must_use]
    pub fn of(&self, object: &Hash) -> Signed {
        Signed {
            object: object.clone(),
            key: self.key.clone(),
            signature: self.signature.clone(),
        }
    }
}

/// Does `endorsement` vouch for `enrolment` from a key `history` trusts for `tapper` under
/// `root`, in every prodrome it holds? Answers the `Signed` to store beside the registration.
/// A pure reading: the tap asks it of the first chain before anything is written.
///
/// # Errors
/// Why not, as a post's refusal says it.
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

/// THE TAP'S DOOR TO KEYS: every chain, and the root that signs what is registered there. Held
/// by no record; only the tap route builds one.
#[derive(Clone, Copy)]
pub struct Enrolling<'a> {
    /// Every chain, the first being the one an endorsement is read against and kept in.
    pub chains: &'a [&'a dyn Prodrome<TodoEvent>],
    /// The root.
    pub root: &'a Secret,
}

impl Enrolling<'_> {
    /// Is `endorsement` a trusted device of `tapper`'s vouching for `enrolment`? Answers what
    /// it signed.
    ///
    /// # Errors
    /// The first chain unreadable; inside, why not.
    pub fn endorsed(
        &self,
        tapper: &Actor,
        enrolment: &Enrolment,
        endorsement: &Endorsement,
    ) -> Result<Result<Signed, Unproven>, StoreError> {
        let Some(first) = self.chains.first() else {
            return Ok(Err(Unproven::Unsigned));
        };
        let history: Dag<TodoEvent> = first.objects()?.into_iter().collect();
        Ok(endorses(
            &history,
            &self.root.public(),
            tapper,
            enrolment,
            endorsement,
        ))
    }

    /// Register the enrolment's key for its actor on every chain, signed by the root, and keep
    /// the endorsement beside the first chain's registration.
    ///
    /// # Errors
    /// A store refusing; the chains written before it keep the key, and tapping again writes
    /// the same objects.
    pub fn enrol(&self, enrolment: &Enrolment, endorsed: &Signed) -> Result<(), StoreError> {
        for chain in self.chains {
            super::trust(*chain, self.root, &enrolment.actor, &enrolment.key)?;
        }
        if let Some(first) = self.chains.first() {
            let object = Envelope::<TodoEvent>::Signed(endorsed.clone());
            first.accept(&[(seal_hash(&object), canonical_envelope(&object))])?;
        }
        Ok(())
    }
}
