//! Who may write: the signatures a chain trusts, the requests a device signed, the chains read as
//! one set, and the devices and epochs they name.

#![forbid(unsafe_code)]

pub mod believed;
pub mod chain_set;
pub mod devices;
pub mod epoch;
pub mod signatures;
pub mod wire {
    //! The web's payloads of the signatures.
    pub mod signatures;
}

// The inert device log: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
