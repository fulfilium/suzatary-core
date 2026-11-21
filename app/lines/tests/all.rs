//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "conversation_id.rs"]
mod conversation_id;
#[path = "frontier.rs"]
mod frontier;
#[path = "line_replica.rs"]
mod line_replica;
#[path = "sequence.rs"]
mod sequence;
#[path = "snapshot.rs"]
mod snapshot;
