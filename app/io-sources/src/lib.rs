//! The interpreters of Drive, the GitHub mirrors, the web, the weather and push.
//!
//! No policy (decisions arrive as values) and no logging (a refusal is a
//! returned value).

#![forbid(unsafe_code)]

pub mod briefings;
pub mod drive;
pub mod notify;
pub mod repos;
pub mod weather;
pub mod web;
