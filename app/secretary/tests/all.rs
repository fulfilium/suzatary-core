//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "calls.rs"]
mod calls;
#[path = "context_cost.rs"]
mod context_cost;
#[path = "filed.rs"]
mod filed;
#[path = "held_values.rs"]
mod held_values;
#[path = "model_port.rs"]
mod model_port;
#[path = "surface.rs"]
mod surface;
#[path = "triage_cards.rs"]
mod triage_cards;
#[path = "triage_memory.rs"]
mod triage_memory;
