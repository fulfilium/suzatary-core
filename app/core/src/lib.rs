//! Suzatary's meaning, pure: no I/O, no async, no clock of its own.
//!
//! Every effect is a trait in `capability`, so the rules they carry (the agent cannot send,
//! loopback cannot write, rendered means confirmed) are types rather than tests.

#![forbid(unsafe_code)]

pub mod agent;
pub mod asks;
pub mod conversation;
pub mod model_name;
pub mod notify;
pub mod release;
pub mod secretary;
pub mod settings {
    //! The settings page's overview, beside `suzatary_connect::settings`.
    pub mod overview;
}
pub mod stores;
pub mod sync;
pub mod verb;
pub mod wire;

// The inert double's impls of this crate's ports, and the fake compactor: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/joining.rs"]
pub mod joining;
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
