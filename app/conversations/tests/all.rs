//! Every integration test of this crate as one binary; a file under `tests/` that is not a
//! module here, or a target of its own in `Cargo.toml`, does not run.

#[path = "attachment.rs"]
mod attachment;
#[path = "held.rs"]
mod held;
#[path = "line_words.rs"]
mod line_words;
