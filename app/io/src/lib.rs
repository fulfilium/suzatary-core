//! The interpreters: the ports' capabilities, run against the world.
//!
//! No policy (decisions arrive as values) and no logging (a refusal is a
//! returned value).

#![forbid(unsafe_code)]

pub mod briefings;
pub mod corpus;
pub mod drive;
pub mod gmail;
pub mod google;
pub mod imap;
pub mod junk;
pub mod mail_sync;
pub mod mailbox;
pub mod notify;
pub mod oauth;
pub mod older;
pub mod pullers;
pub mod repos;
pub mod smtp;
pub mod sync;
pub mod weather;
pub mod web;
