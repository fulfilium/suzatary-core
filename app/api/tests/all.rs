//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "frontiers.rs"]
mod frontiers;
#[path = "held_lines.rs"]
mod held_lines;
#[path = "known_pull.rs"]
mod known_pull;
#[path = "pieces.rs"]
mod pieces;
#[path = "transcript.rs"]
mod transcript;
#[path = "web_vectors.rs"]
mod web_vectors;
