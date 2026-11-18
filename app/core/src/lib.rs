//! Suzatary's meaning, pure: no I/O, no async, no clock of its own.
//!
//! Every effect is a trait in `capability`, so the rules they carry (the agent cannot send,
//! loopback cannot write, rendered means confirmed) are types rather than tests.

#![forbid(unsafe_code)]

pub mod agent;
pub mod asks;
pub mod conversation;
pub mod release;
pub mod secretary;
pub mod stores;
pub mod sync;
pub mod verb;
pub mod wire;

// The inert double's impls of this crate's ports: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
