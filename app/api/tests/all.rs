//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "asked_again.rs"]
mod asked_again;
#[path = "chain_acceptance.rs"]
mod chain_acceptance;
#[path = "door_findings.rs"]
mod door_findings;
#[path = "frontiers.rs"]
mod frontiers;
#[path = "held_lines.rs"]
mod held_lines;
#[path = "known_pull.rs"]
mod known_pull;
#[path = "lines_rule.rs"]
mod lines_rule;
#[path = "own_lines.rs"]
mod own_lines;
#[path = "pieces.rs"]
mod pieces;
#[path = "sealed_decision.rs"]
mod sealed_decision;
#[path = "sequenced.rs"]
mod sequenced;
#[path = "topics.rs"]
mod topics;
#[path = "transcript.rs"]
mod transcript;
