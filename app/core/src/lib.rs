//! Suzatary's meaning, pure: no I/O, no async, no clock of its own.
//!
//! Every effect is a trait in `capability`, so the rules they carry (the agent cannot send,
//! loopback cannot write, rendered means confirmed) are types rather than tests.

#![forbid(unsafe_code)]

pub mod agent;
pub mod attachment;
pub mod authoring;
pub mod believed;
pub mod calendar;
pub mod capability;
pub mod cast;
pub mod chain_set;
pub mod chains;
pub mod clienterror;
pub mod coding;
pub mod conformance;
pub mod connect;
pub mod content_id;
pub mod conversation;
pub mod decisions;
pub mod decoration;
pub mod deployment;
pub mod devices;
pub mod drive;
pub mod enclosed;
pub mod epoch;
pub mod escape;
pub mod excerpt;
pub mod file_read;
pub mod for_model;
pub mod gate;
pub mod google;
pub mod health;
pub mod held_file;
pub mod held_value;
pub mod leak;
pub mod mail;
pub mod model_name;
pub mod narrowing;
pub mod nonempty;
pub mod notify;
#[cfg(test)]
mod own;
pub mod persona;
pub mod plugin;
pub mod portal;
pub mod query;
pub mod reading;
pub mod record;
pub mod redact;
pub mod reference;
pub mod refusal;
pub mod release;
pub mod rendering;
pub mod repos;
pub mod routes;
pub mod rule;
pub mod rule_version;
pub mod secretary;
pub mod settings;
pub mod signatures;
pub mod site;
pub mod source_key;
pub mod store;
pub mod sync;
pub mod table;
pub mod term_json;
pub mod todo;
pub mod todos;
pub mod tree;
pub mod tripwire;
pub mod upload;
pub mod url;
pub mod usage;
pub mod vault;
pub mod verb;
pub mod view;
pub mod weather;
pub mod web;
pub mod wire;
mod words;
pub mod zone;
