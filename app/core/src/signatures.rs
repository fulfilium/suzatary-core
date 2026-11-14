//! Who wrote an object, read from the history: the Prodrome's signatures (SPEC §3, §5, law 42)
//! as this deployment reads them.
//!
//! An object's actor is a field its writer chose; a `Signed` by a key registered to that actor
//! proves it, to anyone holding the objects. Keys are history too: `KeyAdded` and `KeyRevoked`
//! objects in each chain, counted where the deployment's ROOT signed them, so the trusted keys
//! are a register of the store (a set that only grows, and its revocations), never a file.
//!
//! [`Trust`] is that reading, one fold over the object set: what is proven, which is the
//! Prodrome's [`Proof`] exactly (a law, tested), WHY something is not ([`Unproven`]) and what a
//! key's standing is ([`KeyStanding`]). Each signature's check in it is read through the
//! Prodrome's [`Checks`](prodrome_core::sign::Checks), the check tabulated by the signature's
//! name, so a history read again checks only what it gained. History written before the deployment adopted
//! signatures is not unproven but EARLIER: the root's signed `Snapshot` of each chain, its
//! [`epoch`], attests what the box held then, and everything beneath it reads
//! [`Provenance::Predates`].
//!
//! [`Proof`]: prodrome_core::sign::Proof

mod admitted;
mod asking;
mod csrf;
mod custody;
mod enrolment;
mod epoch;
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
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/scene.rs"]
mod scene;
mod signers;
mod tap;
mod trust;
mod trusting;
mod unproven;

pub use admitted::{Admitted, Seat};
pub use asking::{Asking, Possessed, Proven, Request, Trusted, refused};
pub use csrf::{ACT_HEADER, ActHeaders, CLIENT, CLIENT_HEADER, CLIENT_MIN, check_csrf};
pub use custody::Custody;
pub use enrolment::{Endorsement, Enrolling, Enrolment};
pub use epoch::since_epoch;
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
#[cfg(any(test, feature = "nothing"))]
pub use scene::{phone_trusted, scene};
pub use signers::Signers;
pub use tap::Tap;
pub use trust::{KEPT_CHECKS, Trust};
pub use trusting::Trusting;
pub use unproven::Unproven;

#[cfg(test)]
mod tests;
