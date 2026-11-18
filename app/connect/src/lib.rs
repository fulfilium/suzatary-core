//! The deployment's declared connections and settings, their secrets and their checks.

#![forbid(unsafe_code)]

pub mod cast;
pub mod conformance;
pub mod connect;
pub mod deployment;
pub mod secrets;
pub mod settings;

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
