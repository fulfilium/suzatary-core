//! Every integration test of this crate as one binary, so the application links once.
//! A file under `tests/` that is not a module here does not run; `suites.rs` in the cli's tests
//! checks this.

#[path = "memory_schema.rs"]
mod memory_schema;
