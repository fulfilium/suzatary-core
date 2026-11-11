use prodrome_core::event::Actor;
use prodrome_core::sign::{PublicKey, Registrar, Signature, Signed};

use super::{Requested, Trust, Unproven, WINDOW_SECONDS, body_digest, is_nonce, request_name};
use crate::record::TodoEvent;
use crate::store::{Prodrome, StoreError};
use suzatary_base::refusal::Refusal;

/// A DEVICE'S REQUEST AT THE DOOR, as its headers carry it: the parts its name is the hash of,
/// the body read whole, and the key and signature it names, absent where it was not signed.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The method it asked with.
    pub method: &'a str,
    /// The path and query it asked at.
    pub target: &'a str,
    /// The instant its device stamped on it, RFC 3339.
    pub at: Option<&'a str>,
    /// The nonce its device drew for it.
    pub nonce: Option<&'a str>,
    /// The body, whole.
    pub body: &'a [u8],
    /// The device key it names.
    pub key: Option<&'a str>,
    /// Its signature of the request's name.
    pub signature: Option<&'a str>,
}

/// A request named by its content, signed by the key it names, stamped within the window of the
/// box's clock, and written as the gate's actor. [`Asking::possessed`] or [`Asking::trusted`]
/// consumes it.
#[derive(Debug)]
pub struct Asking {
    record: Requested,
    signed: Signed,
    actor: Actor,
}

/// A request whose signature verifies under the key it names: a device showing it holds the key
/// it asks to have trusted, and nothing more.
#[derive(Debug)]
pub struct Possessed(Proven);

/// A request signed by a key the history trusts for its actor on every chain the route can
/// write: the only proof a route that acts for the owner may hold, and the one a
/// [`crate::signatures::Tap`] is minted from.
///
/// It has no constructor but [`Asking::trusted`], which reads the trust, so holding one is the
/// proof that reading passed.
///
/// ```compile_fail
/// let forged = suzatary_core::signatures::Trusted(todo!());
/// ```
#[derive(Debug)]
pub struct Trusted(Proven);

/// What a proven request carries: its record, the key that signed it and the actor it writes as.
#[derive(Debug, Clone)]
pub struct Proven {
    record: Requested,
    device: PublicKey,
    actor: Actor,
}

impl Asking {
    /// The request as the door reads it, written as `actor`, the gate's: refused when it carries
    /// no instant, nonce, key or signature, when its instant is not one or lies outside
    /// [`WINDOW_SECONDS`] of `now`, both read in UTC, or when its key or signature is malformed.
    ///
    /// # Errors
    /// The door's refusal, in its words.
    pub fn of(
        request: &Request<'_>,
        actor: Actor,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Self, Refusal> {
        let unsigned = || refused(&Unproven::Unsigned);
        let at = request.at.ok_or_else(unsigned)?;
        let stamped = chrono::DateTime::parse_from_rfc3339(at)
            .map_err(|_| Refusal::bad_request("the request's instant is not an RFC 3339 instant"))?
            .with_timezone(&chrono::Utc);
        if (stamped - now).num_seconds().abs() > WINDOW_SECONDS {
            return Err(Refusal::forbidden(format!(
                "this request is stamped {at}, more than {} minutes from the box's clock: check \
                 this device's clock and ask again",
                WINDOW_SECONDS / 60
            )));
        }
        let nonce = request.nonce.ok_or_else(unsigned)?;
        if !is_nonce(nonce) {
            return Err(Refusal::bad_request(
                "the request's nonce is 128 random bits in lowercase hex",
            ));
        }
        let (Some(key), Some(signature)) = (request.key, request.signature) else {
            return Err(unsigned());
        };
        let forged = || refused(&Unproven::Forged);
        let key = PublicKey::new(key).map_err(|_| forged())?;
        let signature = Signature::new(signature).map_err(|_| forged())?;
        let name = request_name(
            request.method,
            request.target,
            at,
            nonce,
            &body_digest(request.body),
            key.as_str(),
        );
        Ok(Self {
            record: Requested {
                name: name.as_str().to_owned(),
                method: request.method.to_owned(),
                target: request.target.to_owned(),
                at: at.to_owned(),
                key: key.as_str().to_owned(),
                signature: signature.as_str().to_owned(),
                actor: actor.as_str().to_owned(),
                status: 0,
                wrote: Vec::new(),
            },
            signed: Signed {
                object: name,
                key,
                signature,
            },
            actor,
        })
    }

    /// The request, its signature verifying under the key it names.
    ///
    /// # Errors
    /// [`Unproven::Forged`].
    pub fn possessed(self) -> Result<Possessed, Unproven> {
        if self.signed.verifies() {
            Ok(Possessed(self.proven()))
        } else {
            Err(Unproven::Forged)
        }
    }

    /// The request, its key trusted for its actor on every one of `chains`, each read under
    /// `root`.
    ///
    /// # Errors
    /// Why not, from the first chain that does not trust it; no chains at all trust nothing.
    pub fn trusted(
        self,
        chains: &[&dyn Prodrome<TodoEvent>],
        root: &PublicKey,
    ) -> Result<Trusted, Unproven> {
        let untrusted = || Unproven::Untrusted {
            key: self.signed.key.clone(),
        };
        if chains.is_empty() {
            return Err(untrusted());
        }
        let roots = Registrar::Roots([root.clone()].into_iter().collect());
        for chain in chains {
            let trust = Trust::held(*chain, &roots).map_err(|_| untrusted())?;
            trust.admits(trust.home(), &self.actor, &self.signed)?;
        }
        Ok(Trusted(self.proven()))
    }

    /// [`Unproven::Revoked`] where any of `chains` revoked the key for its actor; asked only where
    /// the signature verifies, since a request naming a key it does not hold proves nothing.
    ///
    /// # Errors
    /// A chain unreadable.
    pub fn revoked(
        &self,
        chains: &[&dyn Prodrome<TodoEvent>],
        root: &PublicKey,
    ) -> Result<Option<Unproven>, StoreError> {
        if !self.signed.verifies() {
            return Ok(None);
        }
        let roots = Registrar::Roots([root.clone()].into_iter().collect());
        for chain in chains {
            let trust = Trust::held(*chain, &roots)?;
            match trust.admits(trust.home(), &self.actor, &self.signed) {
                Err(why @ Unproven::Revoked { .. }) => return Ok(Some(why)),
                Ok(())
                | Err(Unproven::Unsigned | Unproven::Forged | Unproven::Untrusted { .. }) => {}
            }
        }
        Ok(None)
    }

    fn proven(self) -> Proven {
        Proven {
            record: self.record,
            device: self.signed.key,
            actor: self.actor,
        }
    }
}

impl Proven {
    /// The record the box keeps of it once performed.
    #[must_use]
    pub fn record(&self) -> &Requested {
        &self.record
    }

    /// The device key that signed it.
    #[must_use]
    pub fn device(&self) -> &PublicKey {
        &self.device
    }

    /// The actor it writes as, the gate's.
    #[must_use]
    pub fn actor(&self) -> &Actor {
        &self.actor
    }
}

impl Possessed {
    /// What it carries.
    #[must_use]
    pub fn proven(&self) -> &Proven {
        &self.0
    }
}

impl Trusted {
    /// What it carries.
    #[must_use]
    pub fn proven(&self) -> &Proven {
        &self.0
    }
}

/// The door's refusal of a request its signature does not prove: a revoked key's is the wipe,
/// on which the tab deletes what it holds; every other a 403.
#[must_use]
pub fn refused(why: &Unproven) -> Refusal {
    let said = format!("this request's signature does not prove it: {}", why.said());
    match why {
        Unproven::Revoked { .. } => Refusal::wipe(format!(
            "{said}; this device deletes what it holds, and may ask to be trusted again"
        )),
        Unproven::Unsigned | Unproven::Forged | Unproven::Untrusted { .. } => {
            Refusal::forbidden(said)
        }
    }
}
