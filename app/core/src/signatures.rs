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
//! key's standing is ([`KeyStanding`]). Each signature's check in it is read through
//! [`Verified`], the check tabulated by the signature's name, so a history read again checks
//! only what it gained. History written before the deployment adopted
//! signatures is not unproven but EARLIER: the root's signed `Snapshot` of each chain, its
//! [`epoch`], attests what the box held then, and everything beneath it reads
//! [`Provenance::Predates`].
//!
//! [`Proof`]: prodrome_core::sign::Proof

mod asking;
mod custody;
mod enrolment;
mod epoch;
mod key_standing;
mod keys;
mod pairing;
mod provenance;
mod registry;
mod request;
mod requested;
mod signers;
mod trust;
mod unproven;
mod verified;

#[cfg(any(test, feature = "nothing"))]
pub use asking::scene;
pub use asking::{Asking, Possessed, Proven, Request, Trusted, refused};
pub use custody::Custody;
pub use enrolment::{Endorsement, Enrolling, Enrolment, endorses};
pub use epoch::since_epoch;
pub use key_standing::KeyStanding;
pub use keys::{AGENT_KEY, BOX_KEY, BoxKeys, NotTheKeys, agent_key};
#[cfg(any(test, feature = "nothing"))]
pub use pairing::InMemoryPairings;
pub use pairing::{
    ALPHABET, Code, LETTERS, MINUTES, NotACode, Pairing, Pairings, Presented, RUN, Refused, Salt,
    TRIES, present,
};
pub use provenance::Provenance;
pub use registry::{
    AdoptError, Adoption, Epoch, UnsignedWrite, adopt, complete_revocations, epoch, revoke,
    revoke_misplaced, revoked_for, sign_unsigned, trust, trusted_for, unsigned_writes,
};
pub use request::{WINDOW_SECONDS, body_digest, is_nonce, request_name};
pub use requested::{Answer, Requested};
pub use signers::Signers;
pub use trust::Trust;
pub use unproven::Unproven;
pub use verified::{KEPT_CHECKS, Verified};

#[cfg(test)]
mod tests;
