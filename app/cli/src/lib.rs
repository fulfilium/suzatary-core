//! The binary's modules as a library, so its wiring can be integration-tested.

#![forbid(unsafe_code)]

pub mod commands;
pub mod instance;
pub mod paths;
pub mod stamp;
pub mod verb;
pub mod wiring;

pub use commands::serve;
