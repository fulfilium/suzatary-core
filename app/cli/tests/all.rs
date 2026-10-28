//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#![allow(
    clippy::duplicate_mod,
    reason = "each file keeps its own `mod common;`, as it had when it was a crate of its own"
)]

#[path = "accounts.rs"]
mod accounts;
#[path = "acts.rs"]
mod acts;
#[path = "add_topics.rs"]
mod add_topics;
#[path = "adoption.rs"]
mod adoption;
#[path = "attaching.rs"]
mod attaching;
#[path = "attachments.rs"]
mod attachments;
#[path = "bench.rs"]
mod bench;
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
#[path = "migrate_health.rs"]
mod migrate_health;
#[path = "migrate_models.rs"]
mod migrate_models;
#[path = "objects.rs"]
mod objects;
#[path = "paths.rs"]
mod paths;
#[path = "perf_fold_chain.rs"]
mod perf_fold_chain;
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
#[path = "session.rs"]
mod session;
#[path = "sign_in.rs"]
mod sign_in;
#[path = "signatures.rs"]
mod signatures;
#[path = "site.rs"]
mod site;
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
#[path = "table.rs"]
mod table;
#[path = "this_box.rs"]
mod this_box;
#[path = "todos.rs"]
mod todos;
#[path = "typeset.rs"]
mod typeset;
#[path = "web.rs"]
mod web;
#[path = "worker.rs"]
mod worker;
#[path = "writes.rs"]
mod writes;
