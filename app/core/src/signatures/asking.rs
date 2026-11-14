use prodrome_core::event::Actor;
use prodrome_core::sign::{PublicKey, Signature, Signed};

use super::{Requested, Trusting, Unproven, WINDOW_SECONDS, body_digest, is_nonce, request_name};
use suzatary_base::refusal::Refusal;
use suzatary_record::record::TodoEvent;
use suzatary_store::store::{Prodrome, StoreError};

/// A DEVICE'S REQUEST AT THE DOOR, as its headers carry it; key and signature absent where unsigned.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// Its method.
    pub method: &'a str,
    /// Its path and query.
    pub target: &'a str,
    /// Its device's instant, RFC 3339.
    pub at: Option<&'a str>,
    /// Its nonce.
    pub nonce: Option<&'a str>,
    /// The body, whole.
    pub body: &'a [u8],
    /// The device key it names.
    pub key: Option<&'a str>,
    /// The signature of its name.
    pub signature: Option<&'a str>,
}

/// A request named by its content, signed by the key it names, stamped within the window and
/// written as the gate's actor.
#[derive(Debug)]
pub struct Asking {
    record: Requested,
    signed: Signed,
    actor: Actor,
}

/// A request whose signature verifies under the key it names, and nothing more.
#[derive(Debug)]
pub struct Possessed(Proven);

/// A request signed by a key every chain the route can write trusts for its actor: the only
/// proof a route acting for the owner may hold. Only [`Asking::trusted`] makes one.
///
/// ```compile_fail
/// let forged = suzatary_core::signatures::Trusted(todo!());
/// ```
#[derive(Debug)]
pub struct Trusted(Proven);

/// What a proven request carries.
#[derive(Debug, Clone)]
pub struct Proven {
    record: Requested,
    device: PublicKey,
    actor: Actor,
}

impl Asking {
    /// The request as the door reads it, written as `actor`: refused when unsigned, stamped
    /// outside [`WINDOW_SECONDS`] of `now`, or malformed.
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

    /// The request, its key trusted for its actor on every one of `chains` under `root`.
    ///
    /// # Errors
    /// Why not, as [`Trusting::admits`] says it.
    pub fn trusted(
        self,
        chains: &[&dyn Prodrome<TodoEvent>],
        root: &PublicKey,
    ) -> Result<Trusted, Unproven> {
        let trusting = Trusting::held(chains, root).map_err(|_| Unproven::Untrusted {
            key: self.signed.key.clone(),
        })?;
        trusting.admits(&self.actor, &self.signed)?;
        Ok(Trusted(self.proven()))
    }

    /// [`Unproven::Revoked`] where any of `chains` revoked the key; asked only of a verifying signature.
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
        Ok(Trusting::held(chains, root)?.revoked(&self.actor, &self.signed))
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
    /// The record the box keeps once performed.
    #[must_use]
    pub fn record(&self) -> &Requested {
        &self.record
    }

    /// The device key that signed it.
    #[must_use]
    pub fn device(&self) -> &PublicKey {
        &self.device
    }

    /// The gate's actor.
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

/// The door's refusal of an unproven request: the wipe for a revoked key, else a 403.
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
