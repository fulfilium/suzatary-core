//! One turn of the secretary or of the triage agent, pure: no I/O, no async, no clock of its own.
//! A turn takes its role's record of effects (`agent::Agent`, `secretary::Secretary`), so what a
//! role cannot do, such as send mail, is a field its record does not have.

#![forbid(unsafe_code)]

pub mod agent;
pub mod conversation;
pub mod secretary;
pub mod verb;

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
