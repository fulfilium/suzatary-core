//! Every integration test of this crate as one binary: a file under `tests/` that is not a
//! module here, or a target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "table.rs"]
mod table;
#[path = "web_vectors.rs"]
mod web_vectors;
