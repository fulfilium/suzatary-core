//! Every integration test of this crate as one binary, one link instead of one per file; a file
//! under `tests/` that is neither a module here nor its own target does not run, and the cli's
//! `suites.rs` fails then.

#[path = "puller_port.rs"]
mod puller_port;
