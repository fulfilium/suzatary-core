//! The interpreters: `suzatary-core`'s capabilities, run against the world.
//!
//! No policy: every decision about what should happen is made in `suzatary-core` and handed
//! here as a value, and when it happens is the binary's.
//! No logging: a refusal is a value that comes back, and the binary decides what is worth a
//! line.

#![forbid(unsafe_code)]

pub mod attachments;
pub mod blobs;
pub mod blocks;
pub mod briefings;
pub mod calendar;
pub mod clock;
pub mod conversations;
pub mod corpus;
pub mod devices;
pub mod docs;
pub mod drive;
pub mod drive_health;
pub mod durable;
pub mod epochs;
pub mod frontiers;
pub mod gmail;
pub mod google;
pub mod http;
pub mod imap;
pub mod junk;
pub mod lines;
pub mod mail_health;
pub mod mail_sync;
pub mod mailbox;
pub mod manuals;
pub mod model;
pub mod model_health;
pub mod notify;
pub mod oauth;
pub mod objects;
pub mod older;
pub mod pairing;
pub mod pdftotext;
pub mod plugin_store;
pub mod present;
pub mod prompt;
pub mod pullers;
pub mod pulse;
pub mod reading;
pub mod render;
pub mod rendered;
pub mod repos;
pub mod repository;
pub mod requests;
pub mod routine;
pub mod run;
pub mod scratch;
pub mod secrets;
pub mod setup_token;
pub mod smtp;
pub mod state;
pub mod sync;
pub mod text_keys;
pub mod texts;
pub mod tier;
pub mod tokens;
pub mod tool;
pub mod topics;
pub mod typeset;
pub mod unrendered;
pub mod upload_index;
pub mod usage;
pub mod watches;
pub mod weather;
pub mod web;
pub mod whois;
