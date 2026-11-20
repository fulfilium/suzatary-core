//! The plan: the work of every store a deployment reads, as one list ([`listed`]), each task
//! ordered by its reading under the waits ([`waits`]).

#![forbid(unsafe_code)]

pub mod listed;
pub mod waits;
