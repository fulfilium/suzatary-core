//! The one check before every push to the public repository.

use crate::admitted::Admitted;
use crate::fresh::{Fresh, Push};
use crate::leak_check::{CheckError, LeakCheck};
use crate::repo_path::RepoPath;

/// Whether a public tree `T` may be pushed: the leak guard, then what it holds against what
/// core's rows already hold.
#[derive(Debug)]
pub enum Publishable<T> {
    Publishable(T),
    Leak,
    CouldNotRun(CheckError),
    /// The paths the push may not hold until a person admits them.
    Held(Vec<RepoPath>),
}

impl<T> Publishable<T> {
    /// What `train publishable` and `train push-gate` exit with: 0 publishable, 1 a leak, 3 the
    /// check could not run (the status of every error of the tool, which no caller reads as a
    /// verdict), 4 held.
    #[must_use]
    pub fn status(&self) -> u8 {
        match self {
            Publishable::Publishable(_) => 0,
            Publishable::Leak => 1,
            Publishable::CouldNotRun(_) => 3,
            Publishable::Held(_) => 4,
        }
    }
}

/// The verdict on pushing a tree whose leak guard said `check`, which holds `fresh` beside core's
/// rows, to `push`, with the paths a person admitted.
#[must_use]
pub fn publishable<T>(
    check: LeakCheck<T>,
    push: Push,
    fresh: &[Fresh],
    admits: &[Admitted],
) -> Publishable<T> {
    match check {
        LeakCheck::Clean(tree) => match push.held(fresh, admits) {
            held if held.is_empty() => Publishable::Publishable(tree),
            held => Publishable::Held(held),
        },
        LeakCheck::Hits => Publishable::Leak,
        LeakCheck::CouldNotRun(why) => Publishable::CouldNotRun(why),
    }
}
