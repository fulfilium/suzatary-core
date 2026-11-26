//! Every integration test of this crate as one binary: a file under `tests/` that is not a
//! module here or a target in `Cargo.toml` does not run, and `suites.rs` fails when one is missing.

/// Named as `crate::common`: a `mod common;` in each file would type-check it once per file.
mod common;

#[path = "accounts.rs"]
mod accounts;
#[path = "acts.rs"]
mod acts;
#[path = "add_topics.rs"]
mod add_topics;
#[path = "adoption.rs"]
mod adoption;
#[path = "asking.rs"]
mod asking;
#[path = "attaching.rs"]
mod attaching;
#[path = "attachments.rs"]
mod attachments;
#[path = "binary.rs"]
mod binary;
#[path = "briefings.rs"]
mod briefings;
#[path = "calendar.rs"]
mod calendar;
#[path = "clienterrors.rs"]
mod clienterrors;
#[path = "coding.rs"]
mod coding;
#[path = "connections.rs"]
mod connections;
#[path = "context_disk.rs"]
mod context_disk;
#[path = "context_headroom.rs"]
mod context_headroom;
#[path = "decision_tap.rs"]
mod decision_tap;
#[path = "decisions.rs"]
mod decisions;
#[path = "default_conversation.rs"]
mod default_conversation;
#[path = "drive.rs"]
mod drive;
#[path = "erase.rs"]
mod erase;
#[cfg(feature = "fixture")]
#[path = "fixture_live.rs"]
mod fixture_live;
#[path = "follows.rs"]
mod follows;
#[path = "forwarding.rs"]
mod forwarding;
#[path = "frontend.rs"]
mod frontend;
#[path = "gate.rs"]
mod gate;
#[path = "google.rs"]
mod google;
#[path = "heartbeat.rs"]
mod heartbeat;
#[path = "held.rs"]
mod held;
#[path = "husked.rs"]
mod husked;
#[path = "init.rs"]
mod init;
#[path = "instance.rs"]
mod instance;
#[path = "junk.rs"]
mod junk;
#[path = "live_blocks.rs"]
mod live_blocks;
#[path = "mail.rs"]
mod mail;
#[path = "markup.rs"]
mod markup;
#[path = "mcp.rs"]
mod mcp;
#[path = "memory.rs"]
mod memory;
#[cfg(feature = "fixture")]
#[path = "migrate_cost.rs"]
mod migrate_cost;
#[path = "objects.rs"]
mod objects;
#[path = "operator.rs"]
mod operator;
#[path = "overview_cost.rs"]
mod overview_cost;
#[path = "pages.rs"]
mod pages;
#[path = "paths.rs"]
mod paths;
#[path = "perf_fold_chain.rs"]
mod perf_fold_chain;
#[cfg(feature = "fixture")]
#[path = "ports.rs"]
mod ports;
#[cfg(feature = "fixture")]
#[path = "public_chain.rs"]
mod public_chain;
#[path = "public_store.rs"]
mod public_store;
#[path = "push.rs"]
mod push;
#[path = "ran.rs"]
mod ran;
#[path = "reads.rs"]
mod reads;
#[path = "release.rs"]
mod release;
#[path = "repos.rs"]
mod repos;
#[cfg(feature = "fixture")]
#[path = "resident.rs"]
mod resident;
#[path = "review.rs"]
mod review;
#[path = "rewind.rs"]
mod rewind;
#[path = "rounds.rs"]
mod rounds;
#[path = "routes.rs"]
mod routes;
#[path = "secretary.rs"]
mod secretary;
#[path = "secretary_cli.rs"]
mod secretary_cli;
#[cfg(feature = "fixture")]
#[path = "seeds.rs"]
mod seeds;
#[path = "sign_in.rs"]
mod sign_in;
#[path = "signatures.rs"]
mod signatures;
#[path = "stop.rs"]
mod stop;
#[path = "stores.rs"]
mod stores;
#[path = "stream.rs"]
mod stream;
#[path = "suites.rs"]
mod suites;
#[path = "surface.rs"]
mod surface;
#[path = "sweep.rs"]
mod sweep;
#[path = "this_box.rs"]
mod this_box;
#[path = "todos.rs"]
mod todos;
#[path = "typeset.rs"]
mod typeset;
#[path = "warm_erase.rs"]
mod warm_erase;
#[path = "web.rs"]
mod web;
#[path = "worker.rs"]
mod worker;
#[path = "writes.rs"]
mod writes;
