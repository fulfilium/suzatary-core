//! The compactor's task: the digest's rule at its budget's pace
//! (`secretary::compaction::Paced`), woken at startup and whenever a write moves a
//! conversation's lines, and asleep until the instant a hold frees.
//!
//! Startup is a nudge like any other, so the lines already stored are compacted by the same
//! rule at the same pace as new ones: an import is no special path. One coordinator takes each
//! round, so a round hands its idle workers distinct nodes from one reading, and takes again
//! whenever a worker finishes or a nudge comes. A worker whose step failed (the model
//! unreachable, the subscription down to its reserve) keeps its node and waits out a
//! [`Backoff`] of its own that no nudge shortens, doubling from [`RETRY`] to [`RETRY_MOST`]
//! while its failures repeat, while the other workers go on with theirs. Every call was
//! counted before it was made, so retrying spends within the budget like any other step.

use std::num::NonZeroUsize;
use std::sync::Arc;

use suzatary_core::capability::{Compact, Server};
use suzatary_core::secretary::compact_budget::{Budget, Hold};
use suzatary_core::secretary::compacting::Compacting;
use suzatary_core::secretary::compaction::{Claim, Compacted, Paced, Performed, Round};
use tokio::sync::mpsc;

use super::State;
use super::interpreters::CompactorModel;
use crate::instance::Instance;

/// `state` with the compactor's task started over its first chain's stores when `compacting`
/// runs one; off is the kill switch, with no task, and the lines wait.
pub fn started(state: State, instance: &Instance, compacting: Compacting) -> State {
    match compacting {
        Compacting::Off => state,
        Compacting::On {
            model,
            budget,
            workers,
        } => {
            let model = Arc::new(CompactorModel::new(instance, model));
            let caps = state.first.caps;
            state.with_compactor(Compactor::spawn(caps, model, budget, workers))
        }
    }
}

/// How long a first failed step waits before it is tried again.
const RETRY: std::time::Duration = std::time::Duration::from_mins(15);

/// The longest wait between failed steps.
const RETRY_MOST: std::time::Duration = std::time::Duration::from_hours(4);

/// The wait after a failed step: [`RETRY`], doubled by each failure that follows to at most
/// [`RETRY_MOST`], and back to [`RETRY`] once a step compacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Backoff(std::time::Duration);

impl Backoff {
    fn new() -> Self {
        Self(RETRY)
    }

    /// The wait this failure owes, and the next one doubled.
    fn failed(&mut self) -> std::time::Duration {
        let wait = self.0;
        self.0 = (self.0 * 2).min(RETRY_MOST);
        wait
    }
}

/// The compactor's channel: a nudge, never a payload, since every round reads again.
pub struct Compactor {
    nudge: mpsc::Sender<()>,
}

impl Compactor {
    /// Start the task over `caps`'s memory and clock, asking `compact` from `workers` steps at
    /// once, all held to `budget`; it takes its first round at once.
    #[must_use]
    pub fn spawn(
        caps: &'static Server<'static>,
        compact: Arc<dyn Compact + Send + Sync>,
        budget: Budget,
        workers: NonZeroUsize,
    ) -> Self {
        let (nudge, nudged) = mpsc::channel(1);
        tokio::spawn(run(caps, compact, budget, workers, nudged));
        Self { nudge }
    }

    /// A conversation's lines moved: the task takes another round.
    pub fn nudge(&self) {
        // Full is fine: a nudge already stands and the next round reads again.
        let _ = self.nudge.try_send(());
    }
}

/// The coordinator: each round's claims go to idle workers, and a worker that finishes is idle
/// again, which starts the next round.
async fn run(
    caps: &'static Server<'static>,
    compact: Arc<dyn Compact + Send + Sync>,
    budget: Budget,
    workers: NonZeroUsize,
    mut nudged: mpsc::Receiver<()>,
) {
    let paced = Arc::new(Paced::new(compact.as_ref(), budget));
    let (finished, mut free) = mpsc::unbounded_channel();
    let slots: Vec<mpsc::Sender<Claim>> = (0..workers.get())
        .map(|slot| {
            let (hand, handed) = mpsc::channel(1);
            tokio::spawn(work(
                slot,
                caps,
                Arc::clone(&compact),
                handed,
                finished.clone(),
            ));
            hand
        })
        .collect();
    let mut idle: Vec<usize> = (0..workers.get()).collect();
    let mut said: Option<Hold> = None;
    let mut backoff = Backoff::new();
    loop {
        // Drained before the read, so a write landing during the round leaves its nudge.
        let _ = nudged.try_recv();
        let mut wait = None;
        if !idle.is_empty() {
            let (asked, count) = (Arc::clone(&paced), idle.len());
            let round = tokio::task::spawn_blocking(move || {
                asked.take(&caps.secretary.memories, count, caps.secretary.clock)
            })
            .await;
            match round {
                Ok(Ok(Round::Taken(claims))) => {
                    said = None;
                    backoff = Backoff::new();
                    let rest = idle.len().saturating_sub(claims.len());
                    for (slot, claim) in idle.split_off(rest).into_iter().zip(claims) {
                        if slots[slot].send(claim).await.is_err() {
                            return;
                        }
                    }
                }
                Ok(Ok(Round::Held(hold))) => {
                    if said != Some(hold) {
                        tracing::info!("compactor: {hold}");
                        said = Some(hold);
                    }
                    let now = prodrome_core::fpl::instant_of(caps.secretary.clock.now());
                    wait = Some((hold.until() - now).to_std().unwrap_or_default());
                }
                Ok(Ok(Round::Idle)) => {}
                Ok(Err(refused)) => {
                    let wait = backoff.failed();
                    tracing::warn!(
                        "compactor: {}; reading again in {} minutes",
                        refused.message(),
                        wait.as_secs() / 60
                    );
                    tokio::time::sleep(wait).await;
                    continue;
                }
                Err(join_error) => {
                    tracing::warn!(
                        "compactor: a round did not complete, so it stops: {join_error}"
                    );
                    return;
                }
            }
        }
        let held = async {
            match wait {
                Some(until) => tokio::time::sleep(until).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = held => {}
            nudge = nudged.recv() => if nudge.is_none() {
                return;
            },
            slot = free.recv() => match slot {
                Some(slot) => idle.push(slot),
                None => return,
            },
        }
    }
}

/// One worker: each claim handed to it performed, and the worker said idle once it is let go.
async fn work(
    slot: usize,
    caps: &'static Server<'static>,
    compact: Arc<dyn Compact + Send + Sync>,
    mut handed: mpsc::Receiver<Claim>,
    finished: mpsc::UnboundedSender<usize>,
) {
    let mut backoff = Backoff::new();
    while let Some(claim) = handed.recv().await {
        let asked = Arc::clone(&compact);
        let performed = tokio::task::spawn_blocking(move || {
            claim.perform(
                &caps.secretary.memories,
                asked.as_ref(),
                caps.secretary.clock,
            )
        })
        .await;
        match performed {
            Ok(Ok(Performed::Compacted(done))) => {
                tracing::info!("compactor: {}", logged(&done));
                backoff = Backoff::new();
            }
            // The coordinator's next round says the hold and waits it out.
            Ok(Ok(Performed::Held(_))) => {}
            Ok(Err(failed)) => {
                let wait = backoff.failed();
                tracing::warn!(
                    "compactor: {}; worker {slot} tries again in {} minutes",
                    failed.refusal().message(),
                    wait.as_secs() / 60
                );
                // `failed` holds the node through the wait, so no other worker takes it.
                tokio::time::sleep(wait).await;
            }
            Err(join_error) => {
                tracing::warn!(
                    "compactor: a step did not complete, so worker {slot} stops: {join_error}"
                );
                return;
            }
        }
        if finished.send(slot).is_err() {
            return;
        }
    }
}

/// A firing as a log line names it: the node by its first twelve digits.
fn logged(done: &Compacted) -> String {
    let short = |hash: &prodrome_core::event::Hash| hash.as_str()[..12].to_owned();
    match done {
        Compacted::Summarized { node, object } => {
            format!("summarised {} as {}", short(node), short(object))
        }
        Compacted::Unfit { node, object } => {
            format!("found no line for {} ({})", short(node), short(object))
        }
    }
}
