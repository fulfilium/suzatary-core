//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. A file under `tests/` that is not a
//! module here, or a target of its own in `Cargo.toml`, does not run, and
//! `suites.rs` in the cli's tests fails when one is missing.

#[path = "calendar.rs"]
mod calendar;
