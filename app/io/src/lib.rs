//! The interpreters: the ports' capabilities, run against the world.
//!
//! No policy (decisions arrive as values) and no logging (a refusal is a
//! returned value).

#![forbid(unsafe_code)]

pub mod blocks;
pub mod briefings;
pub mod calendar;
pub mod conversations;
pub mod corpus;
pub mod devices;
pub mod drive;
pub mod gmail;
pub mod google;
pub mod ignores;
pub mod imap;
pub mod junk;
pub mod lines;
pub mod mail_sync;
pub mod mailbox;
pub mod model;
pub mod notify;
pub mod oauth;
pub mod objects;
pub mod older;
mod outside;
pub mod plugin_store;
pub mod prompt;
pub mod pullers;
pub mod reading;
pub mod repos;
pub(crate) mod runs;
pub mod setup_token;
pub mod smtp;
pub mod sync;
pub mod topics;
pub mod usage;
pub mod watches;
pub mod weather;
pub mod web;
