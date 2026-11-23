//! This box's files: bytes replaced whole, a cross-process lock, a scratch directory and the
//! box-local tier.

#![forbid(unsafe_code)]

pub mod durable;
pub mod present;
pub mod scratch;
pub mod tier;
