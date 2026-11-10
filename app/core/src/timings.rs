//! How long each phase of a costly read took, as `tracing` events at the target
//! [`TARGET`]. `RUST_LOG=warn,suzatary::timings=info` makes a binary print them to stderr, one
//! plain line per phase.
//!
//! The clock is read only when a subscriber listens at the target, so a build with none, such as
//! the browser's module, where `std::time::Instant` does not exist, never reads it.

use std::time::Instant;

/// The target every timing is logged at.
pub const TARGET: &str = "suzatary::timings";

/// A phase of a costly read, as its timing names it: the secretary's memory, or Settings'
/// overview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Checking that the digest's history and the lines both read whole.
    Whole,
    /// Reading the conversations' line objects.
    Lines,
    /// Reading the lines through their one fold: the words of each line new since the last read
    /// decrypted by its slot's key.
    Opened,
    /// Reading the digest's history objects.
    History,
    /// Folding the lines and the history into the digest: its tree and its summaries.
    Fold,
    /// Reading the standing facts.
    Facts,
    /// Fitting the facts and the record into the agent's answer, which builds a view per probe.
    Remembered,
    /// The agent's whole `context` call.
    Context,
    /// One part of `GET /api/overview`.
    Overview(Part),
}

/// A part of `GET /api/overview`, in the order the route reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The sources' health register.
    Sources,
    /// The model health register.
    Models,
    /// The Drive folders the instance file allows.
    Drive,
    /// The build line: the version and the titles of the todos it completed.
    Build,
    /// The data repository's git configuration.
    Storage,
    /// The Inbox's open connection cards.
    Inbox,
    /// Each declared plugin's store.
    Plugins,
    /// The data repository's reflogs.
    Pushes,
    /// Where each declared secret and each mail grant stands.
    Secrets,
    /// The page drawn from all of the above.
    Drawn,
    /// The whole request.
    Whole,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Whole => "whole",
            Phase::Lines => "lines",
            Phase::Opened => "opened",
            Phase::History => "history",
            Phase::Fold => "fold",
            Phase::Facts => "facts",
            Phase::Remembered => "remembered",
            Phase::Context => "context",
            Phase::Overview(part) => match part {
                Part::Sources => "overview.sources",
                Part::Models => "overview.models",
                Part::Drive => "overview.drive",
                Part::Build => "overview.build",
                Part::Storage => "overview.storage",
                Part::Inbox => "overview.inbox",
                Part::Plugins => "overview.plugins",
                Part::Pushes => "overview.pushes",
                Part::Secrets => "overview.secrets",
                Part::Drawn => "overview.drawn",
                Part::Whole => "overview",
            },
        }
    }
}

/// `run`'s answer, its time logged as `phase`'s when a subscriber listens.
pub fn timed<T>(phase: Phase, run: impl FnOnce() -> T) -> T {
    if !tracing::enabled!(target: "suzatary::timings", tracing::Level::INFO) {
        return run();
    }
    let started = Instant::now();
    let answer = run();
    tracing::info!(
        target: "suzatary::timings",
        phase = phase.name(),
        ms = started.elapsed().as_secs_f64() * 1000.0,
    );
    answer
}

/// `count` of something `phase` did, such as the views it built, logged when a subscriber
/// listens.
pub fn counted(phase: Phase, what: &'static str, count: usize) {
    tracing::info!(target: "suzatary::timings", phase = phase.name(), what, count);
}
