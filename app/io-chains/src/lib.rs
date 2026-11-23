//! The chains on disk: a chain's object store (`objects`) and the readings over it, the
//! conversations' lines and their runs, devices, topics, the plugins' stores and the calendar
//! kept in one, the ignore lists and the watches.

#![forbid(unsafe_code)]

pub mod calendar;
pub mod conversations;
pub mod devices;
pub mod ignores;
pub mod lines;
pub mod objects;
mod outside;
pub mod plugin_store;
pub mod reading;
pub(crate) mod runs;
pub mod topics;
pub mod watches;
