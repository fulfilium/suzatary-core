//! The sources read by grant, beside mail: Drive, repositories, usage, web, weather and plugins.

#![forbid(unsafe_code)]

pub mod drive;
pub mod plugin;
pub mod repos;
pub mod repositories;
pub mod repository;
pub mod usage;
pub mod weather;
pub mod web;

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
