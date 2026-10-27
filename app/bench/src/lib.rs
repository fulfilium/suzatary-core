//! The triage bench: case types, their pure extraction from recorded decisions, a replay
//! runner, sampling and cost arithmetic; the CLI's `bench` verb is the effectful shell.

#![forbid(unsafe_code)]

pub mod bar;
pub mod cases;
pub mod cost;
pub mod extract;
pub mod frozen;
pub mod join;
pub mod pool;
pub mod privacy;
pub mod run;
pub mod sample;
pub mod score;
pub mod sheet;
pub mod stratum;
pub mod tally;
