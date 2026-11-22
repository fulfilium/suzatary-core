//! Every integration test of this crate as one binary: a file under `tests/` that
//! is not a module here, or a target of its own in `Cargo.toml`, does not run, and
//! `suites.rs` in the cli's tests fails when one is missing.

#[path = "failing.rs"]
mod failing;
#[path = "fixes.rs"]
mod fixes;
#[path = "lapsed.rs"]
mod lapsed;
#[path = "noticed.rs"]
mod noticed;
#[path = "settings_overview.rs"]
mod settings_overview;
#[path = "undo.rs"]
mod undo;
