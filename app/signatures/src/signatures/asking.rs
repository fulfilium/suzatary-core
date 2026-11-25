use prodrome_data::event::Actor;
use prodrome_data::sign::{PublicKey, Signature, Signed};

use super::{
    Keeper, Keeping, Requested, Trusting, Unproven, Unread, WINDOW_SECONDS, body_digest, is_nonce,
    request_name,
};
use suzatary_base::refusal::Refusal;

/// A device's request at the door, as its headers carry it.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub method: &'a str,
    /// The path and query.
    pub target: &'a str,
    /// The device's instant, RFC 3339.
    pub at: Option<&'a str>,
    pub nonce: Option<&'a str>,
    /// The body, whole.
    pub body: &'a [u8],
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

/// A request signed by a key every keyed chain the route can reach trusts for its actor: the only
/// proof a route acting for the owner may hold. Only [`Asking::trusted`] makes one.
///
/// ```compile_fail
/// let forged = suzatary_signatures::signatures::Trusted(todo!());
/// ```
#[derive(Debug)]
pub struct Trusted(Proven);

#[derive(Debug, Clone)]
pub struct Proven {
    record: Requested,
    device: PublicKey,
    actor: Actor,
}

impl Asking {
    /// Refused when unsigned, malformed, or stamped outside [`WINDOW_SECONDS`] of `now`.
    pub fn of(
        request: &Request<'_>,
        actor: Actor,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Self, Refusal> {
        let unsigned = || refused(&Unproven::Unsigned.into());
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
        let forged = || refused(&Unproven::Forged.into());
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
    pub fn possessed(self) -> Result<Possessed, Unproven> {
        if self.signed.verifies() {
            Ok(Possessed(self.proven()))
        } else {
            Err(Unproven::Forged)
        }
    }

    /// The request, its key trusted for its actor on every keyed chain and revoked on none.
    pub fn trusted(
        self,
        chains: &Keeping<Keeper<'_>>,
        root: &PublicKey,
    ) -> Result<Trusted, Unadmitted> {
        if let Keeping::Keyless = chains {
            return Err(Unadmitted::Keyless);
        }
        Trusting::held(chains, root)
            .map_err(Unadmitted::Unread)?
            .admits(&self.actor, &self.signed)?;
        Ok(Trusted(self.proven()))
    }

    /// [`Unproven::Revoked`] where any keyed chain revoked the key; `None` for a forged signature.
    pub fn revoked(
        &self,
        chains: &Keeping<Keeper<'_>>,
        root: &PublicKey,
    ) -> Result<Option<Unproven>, Unread> {
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
    #[must_use]
    pub fn record(&self) -> &Requested {
        &self.record
    }

    #[must_use]
    pub fn device(&self) -> &PublicKey {
        &self.device
    }

    #[must_use]
    pub fn actor(&self) -> &Actor {
        &self.actor
    }
}

impl Possessed {
    #[must_use]
    pub fn proven(&self) -> &Proven {
        &self.0
    }
}

impl Trusted {
    #[must_use]
    pub fn proven(&self) -> &Proven {
        &self.0
    }
}

/// Why the door admits no request.
#[derive(Debug)]
pub enum Unadmitted {
    Unproven(Unproven),
    Unread(Unread),
    /// No chain the request is checked against keeps keys.
    Keyless,
}

impl From<Unproven> for Unadmitted {
    fn from(why: Unproven) -> Self {
        Unadmitted::Unproven(why)
    }
}

/// The door's refusal: the wipe for a revoked key, a 500 for a chain unread, else a 403.
#[must_use]
pub fn refused(why: &Unadmitted) -> Refusal {
    match why {
        Unadmitted::Unproven(unproven) => {
            let said = format!(
                "this request's signature does not prove it: {}",
                unproven.said()
            );
            match unproven {
                Unproven::Revoked { .. } => Refusal::wipe(format!(
                    "{said}; this device deletes what it holds, and may ask to be trusted again"
                )),
                Unproven::Unsigned | Unproven::Forged | Unproven::Untrusted { .. } => {
                    Refusal::forbidden(said)
                }
            }
        }
        Unadmitted::Unread(unread) => {
            Refusal::internal(format!("this request could not be checked: {unread}"))
        }
        Unadmitted::Keyless => Refusal::forbidden(
            "this request cannot be proven: no chain it is checked against keeps keys",
        ),
    }
}
