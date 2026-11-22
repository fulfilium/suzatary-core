//! Every integration test of this crate as one binary: a file under `tests/` that is not a
//! module here, or a target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "act_literal.rs"]
mod act_literal;
#[path = "card_stages.rs"]
mod card_stages;
