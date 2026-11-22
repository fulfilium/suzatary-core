//! Who may write: trusted signatures, signed device requests, chains read as one set, devices and
//! epochs.

#![forbid(unsafe_code)]

pub mod believed;
pub mod chain_set;
pub mod devices;
pub mod epoch;
pub mod signatures;
pub mod wire {
    pub mod signatures;
}

// Test support, never production.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
