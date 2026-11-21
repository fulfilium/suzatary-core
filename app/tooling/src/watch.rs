//! The tree half's run on GitHub's runners, watched by asking GitHub once per poll: a pure part of
//! the gate's `step`.

use crate::gating::Effect;
use crate::oid::Oid;
use crate::{Ended, Outcome};

/// A run of `gate.yml`, and its address.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunRef {
    pub id: u64,
    pub url: String,
}

/// A job of the run whose `red` step failed: its gate found the tree red.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RedJob {
    pub id: u64,
    pub name: String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RunStatus {
    Completed(String),
    Going(String),
}

/// One call to GitHub's API.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Ask {
    /// The gate run of the pushed commit.
    Find(Oid),
    Status(RunRef),
    Jobs(RunRef),
    Log(RedJob),
}

/// What a call answered, `None` where it failed. Parsed at the edge, from `gh`'s output.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Answer {
    Run(Option<RunRef>),
    Status(Option<RunStatus>),
    Jobs(Option<Vec<RedJob>>),
    Log(Option<String>),
}

/// Calls in a row that may find nothing, or fail, before the half says nothing.
const MISSES: u8 = 10;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Watch {
    Finding {
        tries: u8,
    },
    Watching {
        run: RunRef,
        misses: u8,
        polls: u32,
    },
    /// The status was read; its jobs are asked next, so a run read as ended has every job's
    /// verdict in what they answer.
    Jobs {
        run: RunRef,
        status: RunStatus,
        misses: u8,
        polls: u32,
    },
    Reading {
        left: Vec<RedJob>,
    },
}

/// What the watch does after an answer.
pub enum Next {
    /// Asks again after a poll.
    Wait(Watch),
    /// Asks at once.
    Ask(Watch),
    Ended(Ended),
}

impl Watch {
    pub const START: Watch = Watch::Finding { tries: 0 };

    #[must_use]
    pub fn ask(&self, pushed: &Oid) -> Ask {
        match self {
            Watch::Finding { .. } => Ask::Find(pushed.clone()),
            Watch::Watching { run, .. } => Ask::Status(run.clone()),
            Watch::Jobs { run, .. } => Ask::Jobs(run.clone()),
            Watch::Reading { left } => Ask::Log(left[0].clone()),
        }
    }

    /// The next move after `answer`, with `limit` polls in all (the six hours a GitHub job may
    /// run), for the run of the gate of `short`.
    pub fn answer(self, answer: Answer, limit: u32, short: &str, fx: &mut Vec<Effect>) -> Next {
        let silent = |fx: &mut Vec<Effect>, why: String| {
            fx.push(Effect::Say(format!(
                "{why}, so the tree half says nothing of {short}"
            )));
            Next::Ended(Ended::Unjudged)
        };
        match (self, answer) {
            (Watch::Finding { .. }, Answer::Run(Some(run))) => {
                fx.push(Effect::Say(format!("waiting on {}", run.url)));
                fx.push(Effect::Found(run.clone()));
                Next::Wait(Watch::Watching {
                    run,
                    misses: 0,
                    polls: 0,
                })
            }
            (Watch::Finding { tries }, Answer::Run(None)) if tries + 1 < MISSES => {
                Next::Wait(Watch::Finding { tries: tries + 1 })
            }
            (Watch::Finding { .. }, _) => silent(
                fx,
                "no run of .github/workflows/gate.yml started on core for the gate commit (are \
                 Actions enabled? scripts/core-settings.sh)"
                    .into(),
            ),
            (Watch::Watching { run, misses, polls }, Answer::Status(Some(status))) => {
                Next::Ask(Watch::Jobs {
                    run,
                    status,
                    misses,
                    polls,
                })
            }
            (Watch::Jobs { .. }, Answer::Jobs(Some(red))) if !red.is_empty() => {
                Next::Ask(Watch::Reading { left: red })
            }
            (
                Watch::Jobs {
                    run, status, polls, ..
                },
                Answer::Jobs(Some(_)),
            ) => match status {
                RunStatus::Completed(how) if how == "success" => {
                    fx.push(Effect::AppendLog(format!("run {}: success\n", run.id)));
                    Next::Ended(Ended::Judged(Outcome::Green))
                }
                RunStatus::Completed(how) => silent(
                    fx,
                    format!(
                        "the run {} ended {how}, and no job's gate found the tree red (a runner \
                         lost, a cancel, a gate that did not start)",
                        run.id
                    ),
                ),
                RunStatus::Going(how) => again(run, 0, polls, limit, &how, short, fx),
            },
            (
                Watch::Watching { run, misses, polls }
                | Watch::Jobs {
                    run, misses, polls, ..
                },
                _,
            ) => again(run, misses + 1, polls, limit, "unreachable", short, fx),
            (Watch::Reading { mut left }, answer) => {
                let job = left.remove(0);
                let log = match answer {
                    Answer::Log(Some(log)) => log,
                    _ => "(its log could not be read)\n".into(),
                };
                fx.push(Effect::Say(format!("red: {}", job.name)));
                fx.push(Effect::AppendLog(format!("--- job {}\n{log}", job.name)));
                if left.is_empty() {
                    Next::Ended(Ended::Judged(Outcome::Red))
                } else {
                    Next::Ask(Watch::Reading { left })
                }
            }
        }
    }
}

/// Another poll of `run`, unless `misses` calls in a row failed or the polls ran out.
fn again(
    run: RunRef,
    misses: u8,
    polls: u32,
    limit: u32,
    how: &str,
    short: &str,
    fx: &mut Vec<Effect>,
) -> Next {
    if misses < MISSES && polls + 1 < limit {
        return Next::Wait(Watch::Watching {
            run,
            misses,
            polls: polls + 1,
        });
    }
    fx.push(Effect::Say(format!(
        "the run {} did not end ({how}), so the tree half says nothing of {short}",
        run.id
    )));
    Next::Ended(Ended::Unjudged)
}
