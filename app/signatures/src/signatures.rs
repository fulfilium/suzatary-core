//! Who wrote an object: the Prodrome's signatures (SPEC §3, §5, law 42) as this deployment reads
//! them.
//!
//! An object's actor is proven by a `Signed` from a key registered to that actor. The trusted keys
//! are `KeyAdded` and `KeyRevoked` objects in each chain that the deployment's root signed, never a
//! file. [`Trust`] folds them and equals the Prodrome's [`Proof`] (a tested law). Objects beneath
//! the root's signed `Snapshot` of a chain, its [`epoch`], read [`Provenance::Predates`].
//!
//! [`Proof`]: prodrome_dag::sign::Proof

mod admitted;
mod asking;
mod csrf;
mod custody;
mod enrolment;
mod epoch;
mod keeping;
mod key_standing;
mod keys;
mod pairing;
// Test support, never production.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/pairings.rs"]
mod pairings;
mod provenance;
mod registry;
mod request;
mod requested;
mod signers;
mod tap;
mod trust;
mod trusting;
mod unproven;

pub use admitted::{Admitted, Seat};
pub use asking::{Asking, Possessed, Proven, Request, Trusted, Unadmitted, refused};
pub use csrf::{ACT_HEADER, ActHeaders, CLIENT, CLIENT_HEADER, CLIENT_MIN, check_csrf};
pub use custody::Custody;
pub use enrolment::{Endorsement, Enrolling, Enrolment};
pub use epoch::{findings_of, since_epoch};
pub use keeping::{Keeper, Keeping, Keyed, Unread};
pub use key_standing::KeyStanding;
pub use keys::{AGENT_KEY, BOX_KEY, BoxKeys, NotTheKeys, agent_key};
pub use pairing::{
    ALPHABET, Code, MINUTES, Pairing, Pairings, Presented, Refused, Salt, TRIES, present,
};
#[cfg(any(test, feature = "nothing"))]
pub use pairings::InMemoryPairings;
pub use provenance::Provenance;
pub use registry::{
    Adoption, UnsignedWrite, adopt, complete_revocations, epoch, revoke, revoke_misplaced,
    revoked_for, sign_unsigned, trust, trusted_for, unsigned_writes,
};
pub use request::{WINDOW_SECONDS, body_digest, is_nonce, request_name};
pub use requested::{Answer, Requested};
pub use signers::Signers;
pub use tap::Tap;
pub use trust::{KEPT_CHECKS, Trust};
pub use trusting::Trusting;
pub use unproven::Unproven;
