//! Every integration test of this crate as one binary; a file under `tests/` that is not a
//! module here, or a target of its own in `Cargo.toml`, does not run.

#[path = "conversation_id.rs"]
mod conversation_id;
#[path = "frontier.rs"]
mod frontier;
#[path = "line_replica.rs"]
mod line_replica;
#[path = "printed.rs"]
mod printed;
#[path = "sequence.rs"]
mod sequence;
#[path = "snapshot.rs"]
mod snapshot;
