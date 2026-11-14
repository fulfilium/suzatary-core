//! Suzatary's meaning, pure: no I/O, no async, no clock of its own.
//!
//! Every effect is a trait in `capability`, so the rules they carry (the agent cannot send,
//! loopback cannot write, rendered means confirmed) are types rather than tests.

#![forbid(unsafe_code)]

pub mod agent;
pub mod asks;
pub mod attachment;
pub mod authoring;
pub mod believed;
pub mod calendar;
pub mod cast;
pub mod chain_set;
pub mod coding;
pub mod coding_report;
pub mod conformance;
pub mod connect;
pub mod content_id;
pub mod conversation;
pub mod decisions;
pub mod deployment;
pub mod devices;
pub mod drive;
pub mod enclosed;
pub mod epoch;
pub mod excerpt;
pub mod file_read;
pub mod for_model;
pub mod google;
pub mod health;
pub mod held_file;
pub mod held_value;
pub mod leak;
pub mod mail;
pub mod mail_error;
pub mod message_part;
pub mod model_name;
pub mod nonempty;
pub mod notify;
pub mod outcome;
pub mod pdf_text;
pub mod plugin;
pub mod priced;
pub mod reading;
pub mod receipt;
pub mod redact;
pub mod release;
pub mod render;
pub mod render_cache;
pub mod rendering;
pub mod repos;
pub mod repositories;
pub mod repository;
pub mod rule;
pub mod secretary;
pub mod secrets;
pub mod settings;
pub mod signatures;
pub mod stores;
pub mod sync;
pub mod template;
pub mod timings;
pub mod todos;
pub mod upload;
pub mod upload_index;
pub mod url;
pub mod usage;
pub mod vault;
pub mod verb;
pub mod view;
pub mod weather;
pub mod web;
pub mod wire;

// The inert double's impls of this crate's ports, and the fake compactor: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/joining.rs"]
pub mod joining;
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
