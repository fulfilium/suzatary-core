//! Suzatary's meaning, pure: no I/O, no async, no clock of its own.
//!
//! Every effect is a trait in `capability`, so the rules they carry (the agent cannot send,
//! loopback cannot write, rendered means confirmed) are types rather than tests.

#![forbid(unsafe_code)]

pub mod agent;
pub mod asks;
pub mod calendar;
pub mod cast;
pub mod coding;
pub mod conformance;
pub mod connect;
pub mod content_id;
pub mod conversation;
pub mod deployment;
pub mod drive;
pub mod first_read;
pub mod google;
pub mod model_name;
pub mod nonempty;
pub mod notify;
pub mod parts;
pub mod plugin;
pub mod priced;
pub mod release;
pub mod repos;
pub mod repositories;
pub mod repository;
pub mod secretary;
pub mod secrets;
pub mod settings;
pub mod stores;
pub mod sync;
pub mod timings;
pub mod url;
pub mod usage;
pub mod verb;
pub mod weather;
pub mod web;
pub mod wire;

// The inert double's impls of this crate's ports, and the fake compactor: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/joining.rs"]
pub mod joining;
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
