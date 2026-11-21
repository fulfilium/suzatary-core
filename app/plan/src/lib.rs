//! The plan: the work of every store a deployment reads, as one list ([`listed`]), each task
//! ordered by its reading under the waits ([`waits`]), each wait read by one rule ([`wait`]).

#![forbid(unsafe_code)]

pub mod listed;
pub mod wait;
pub mod waits;
