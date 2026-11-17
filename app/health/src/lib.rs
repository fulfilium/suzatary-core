//! What went wrong with a source, and what remedy fixes it: each source's health, the classes
//! of failure, the limits a provider set and a plugin's names.

#![forbid(unsafe_code)]

pub mod health;

// The inert register: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
