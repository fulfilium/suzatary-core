//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "act_lines.rs"]
mod act_lines;
#[path = "calendar_reading.rs"]
mod calendar_reading;
#[path = "card_words.rs"]
mod card_words;
#[path = "chain_set.rs"]
mod chain_set;
#[path = "conversational_rules.rs"]
mod conversational_rules;
#[path = "digest.rs"]
mod digest;
#[path = "digest_order.rs"]
mod digest_order;
#[path = "held_lines_fold.rs"]
mod held_lines_fold;
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
#[path = "memories.rs"]
mod memories;
#[path = "memory.rs"]
mod memory;
#[path = "oauth_tap.rs"]
mod oauth_tap;
#[path = "object_replica.rs"]
mod object_replica;
#[path = "opened.rs"]
mod opened;
#[path = "operator_tap.rs"]
mod operator_tap;
#[path = "set_up.rs"]
mod set_up;
#[path = "signatures.rs"]
mod signatures;
#[path = "store_learnt.rs"]
mod store_learnt;
#[path = "store_memoised.rs"]
mod store_memoised;
#[path = "store_signing.rs"]
mod store_signing;
#[path = "tap.rs"]
mod tap;
#[path = "title.rs"]
mod title;
#[path = "transcript_store.rs"]
mod transcript_store;
