//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "act_lines.rs"]
mod act_lines;
#[path = "act_literal.rs"]
mod act_literal;
#[path = "attachment.rs"]
mod attachment;
#[path = "calendar_reading.rs"]
mod calendar_reading;
#[path = "calls.rs"]
mod calls;
#[path = "card_stages.rs"]
mod card_stages;
#[path = "card_words.rs"]
mod card_words;
#[path = "chain_set.rs"]
mod chain_set;
#[path = "connect.rs"]
mod connect;
#[path = "context_cost.rs"]
mod context_cost;
#[path = "conversation_id.rs"]
mod conversation_id;
#[path = "conversational_rules.rs"]
mod conversational_rules;
#[path = "decisions_schema.rs"]
mod decisions_schema;
#[path = "digest.rs"]
mod digest;
#[path = "drive_port.rs"]
mod drive_port;
#[path = "failing.rs"]
mod failing;
#[path = "filed.rs"]
mod filed;
#[path = "fixes.rs"]
mod fixes;
#[path = "fragment.rs"]
mod fragment;
#[path = "held.rs"]
mod held;
#[path = "held_lines_fold.rs"]
mod held_lines_fold;
#[path = "held_values.rs"]
mod held_values;
#[path = "holder.rs"]
mod holder;
#[path = "inbox.rs"]
mod inbox;
#[path = "inbox_prices.rs"]
mod inbox_prices;
#[path = "inbox_schema.rs"]
mod inbox_schema;
#[path = "inbox_store.rs"]
mod inbox_store;
#[path = "lapsed.rs"]
mod lapsed;
#[path = "laws.rs"]
mod laws;
#[path = "line_replica.rs"]
mod line_replica;
#[path = "line_words.rs"]
mod line_words;
#[path = "lines_rule.rs"]
mod lines_rule;
#[path = "memories.rs"]
mod memories;
#[path = "memory.rs"]
mod memory;
#[path = "memory_schema.rs"]
mod memory_schema;
#[path = "model_port.rs"]
mod model_port;
#[path = "noticed.rs"]
mod noticed;
#[path = "oauth_tap.rs"]
mod oauth_tap;
#[path = "object_replica.rs"]
mod object_replica;
#[path = "opened.rs"]
mod opened;
#[path = "operator_tap.rs"]
mod operator_tap;
#[path = "puller_port.rs"]
mod puller_port;
#[path = "release.rs"]
mod release;
#[path = "sequence.rs"]
mod sequence;
#[path = "set_up.rs"]
mod set_up;
#[path = "settings_overview.rs"]
mod settings_overview;
#[path = "signatures.rs"]
mod signatures;
#[path = "snapshot.rs"]
mod snapshot;
#[path = "store_learnt.rs"]
mod store_learnt;
#[path = "store_memoised.rs"]
mod store_memoised;
#[path = "store_signing.rs"]
mod store_signing;
#[path = "surface.rs"]
mod surface;
#[path = "tap.rs"]
mod tap;
#[path = "title.rs"]
mod title;
#[path = "topics.rs"]
mod topics;
#[path = "transcript_store.rs"]
mod transcript_store;
#[path = "transport_port.rs"]
mod transport_port;
#[path = "tree.rs"]
mod tree;
#[path = "triage_cards.rs"]
mod triage_cards;
#[path = "triage_memory.rs"]
mod triage_memory;
#[path = "undo.rs"]
mod undo;
#[path = "vault.rs"]
mod vault;
