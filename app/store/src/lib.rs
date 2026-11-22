//! A content-addressed, signed object store and its replica, for any schema.

#![forbid(unsafe_code)]

pub mod admission;
pub mod receipt;
pub mod replica;
pub mod snapshot;
pub mod store;
pub mod wire {
    //! The web's payloads of a store.
    pub mod store;
}

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/laws.rs"]
pub mod laws;
