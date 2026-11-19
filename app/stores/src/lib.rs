//! Every store the data tree keeps, as one table over the schemas of the crates below, and the
//! asks store, whose events name that table's rows.

#![forbid(unsafe_code)]

pub mod asks;
pub mod stores;
