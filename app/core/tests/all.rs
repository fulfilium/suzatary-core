//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "act_literal.rs"]
mod act_literal;
#[path = "calls.rs"]
mod calls;
#[path = "card_words.rs"]
mod card_words;
#[path = "connect.rs"]
mod connect;
#[path = "decisions_schema.rs"]
mod decisions_schema;
#[path = "digest.rs"]
mod digest;
#[path = "drive_port.rs"]
mod drive_port;
#[path = "frontiers.rs"]
mod frontiers;
#[path = "held.rs"]
mod held;
#[path = "held_lines.rs"]
mod held_lines;
#[path = "held_values.rs"]
mod held_values;
#[path = "inbox.rs"]
mod inbox;
#[path = "inbox_prices.rs"]
mod inbox_prices;
#[path = "inbox_schema.rs"]
mod inbox_schema;
#[path = "laws.rs"]
mod laws;
#[path = "memory.rs"]
mod memory;
#[path = "memory_schema.rs"]
mod memory_schema;
#[path = "model_port.rs"]
mod model_port;
#[path = "pieces.rs"]
mod pieces;
#[path = "puller_port.rs"]
mod puller_port;
#[path = "release.rs"]
mod release;
#[path = "surface.rs"]
mod surface;
#[path = "topics.rs"]
mod topics;
#[path = "transcript.rs"]
mod transcript;
#[path = "transport_port.rs"]
mod transport_port;
#[path = "tree.rs"]
mod tree;
#[path = "triage_cards.rs"]
mod triage_cards;
#[path = "web_vectors.rs"]
mod web_vectors;
