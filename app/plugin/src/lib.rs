//! The plugin host: it runs a component of `app/wit/plugin.wit`'s `source` world with its grant.
//!
//! One [`Host`] holds one wasmtime `Engine` and one `Linker`. The linker holds `fetch`, which
//! crosses `suzatary_core::plugin::Target::admit`, and the WASI clocks, random numbers and
//! standard streams; it holds no WASI filesystem or sockets, so [`Host::load`] refuses a
//! component that imports either. A credential is resolved here by name and never handed in.
//!
//! Each call is a fresh `Store` and instance under a [`Budget`] of time and memory, with an empty
//! environment, an empty standard input and a bounded standard error; a plugin that exceeds a
//! bound is stopped with a [`RunError`] and the host carries on. Nothing a plugin computes
//! survives a call except the cursor it returns.

#![forbid(unsafe_code)]

mod budget;
mod call;
mod clock;
mod host;
mod run;

pub use budget::Budget;
pub use host::{Host, HostError, LoadError, Source};
pub use run::{Effects, Fetch, Run, RunError, Synced};
