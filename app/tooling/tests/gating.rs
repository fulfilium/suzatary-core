//! The laws of a gate run, over `gating::step` alone: laws 2 (the stop) and 3 (a red half stops
//! the other) and the one worktree, over generated interleavings of what the runner can send, and
//! law 4 (bisect) over generated tables. Each effect is witnessed once on real processes by the
//! train guards (`app/cli/tests/guards/train.rs`).

use proptest::prelude::*;
use rustix::process::Signal;
use suzatary_tooling::bisect::{Search, Stuck};
use suzatary_tooling::gate_branch::GateBranch;
use suzatary_tooling::gating::{
    self, Alarm, Dispatch, Done, Effect, Exit, Gating, Job, Msg, OnRed, Piece, Plan, Push, Runs,
    Step, Steps,
};
use suzatary_tooling::oid::Oid;
use suzatary_tooling::outcome::Check;
use suzatary_tooling::supervisor::Status;
use suzatary_tooling::watch::{Answer, Ask, RedJob, RunRef, RunStatus};
use suzatary_tooling::{Ended, Half, Outcome, Reading};

fn oid(n: u8) -> Oid {
    Oid::parse(&format!("{n:040x}")).expect("an oid")
}

fn push() -> Push {
    Push {
        branch: GateBranch::parse(&format!("gate/{}", oid(1))).expect("a branch"),
        subject: "Gate".into(),
        trailer: "Gate: x".into(),
    }
}

/// Every plan the verbs make.
fn plans() -> Vec<Plan> {
    vec![
        Plan::local(oid(1), &Half::BOTH, OnRed::Stop),
        Plan::local(oid(1), &Half::BOTH, OnRed::GoOn),
        Plan::local(oid(1), &[Half::Live], OnRed::Stop),
        Plan::remote(oid(1), push(), true, OnRed::Stop, 4),
        Plan::remote(oid(1), push(), true, OnRed::GoOn, 4),
        Plan::remote(oid(1), push(), false, OnRed::Stop, 4),
        Plan::check(oid(1), push(), 4),
        // No plan the verbs make watches with nothing pushed; the type allows it.
        Plan {
            tree: vec![
                Step {
                    piece: Piece::Gate(Steps::All),
                    after: None,
                },
                Step {
                    piece: Piece::Watch,
                    after: None,
                },
            ],
            ..Plan::local(oid(1), &[], OnRed::Stop)
        },
    ]
}

fn run() -> RunRef {
    RunRef {
        id: 7,
        url: "https://github.test/7".into(),
    }
}

/// One of the ends the runner can deliver for `job`, chosen by `pick`: one time in five the end
/// of a job whose status nothing read; for a run that never appears (`lost`), every lookup
/// answers nothing.
fn end_of(half: Half, job: &Job, pick: u8, lost: bool) -> Msg {
    let red = || RedJob {
        id: 9,
        name: "gate (tests)".into(),
    };
    let polled = Msg::Polled;
    if pick % 5 == 4 {
        return gating::unread(half, job);
    }
    match job {
        Job::Gate(_) => Msg::Ended(
            half,
            Exit::Gated(
                [
                    Ended::Judged(Outcome::Green),
                    Ended::Judged(Outcome::Green),
                    Ended::Judged(Outcome::Red),
                    Ended::Unjudged,
                ][usize::from(pick % 4)],
            ),
        ),
        Job::Check => Msg::Ended(
            half,
            Exit::Checked(
                [
                    Ok(()),
                    Ok(()),
                    Ok(()),
                    Err(Check::Leak),
                    Err(Check::Held),
                    Err(Check::Unrun),
                ][usize::from(pick % 6)],
            ),
        ),
        Job::Push(_) => Msg::Ended(
            half,
            Exit::Pushed(match pick % 6 {
                0 => Err(Check::Held),
                1 => Err(Check::Unrun),
                _ => Ok(oid(2)),
            }),
        ),
        Job::Dispatch(_) => Msg::Ended(
            half,
            Exit::Dispatched(
                [Dispatch::Started, Dispatch::Unanswered, Dispatch::Failed][usize::from(pick % 3)],
            ),
        ),
        Job::Publication(_) => Msg::Ended(half, Exit::Published("ended".into())),
        Job::Ask(Ask::Find(_)) => polled(Answer::Run((!lost && !pick.is_multiple_of(4)).then(run))),
        Job::Ask(Ask::Status(_)) => polled(Answer::Status(match pick % 4 {
            0 => None,
            1 => Some(RunStatus::Going("in_progress".into())),
            2 => Some(RunStatus::Completed("success".into())),
            _ => Some(RunStatus::Completed("cancelled".into())),
        })),
        Job::Ask(Ask::Jobs(_)) => polled(Answer::Jobs(match pick % 3 {
            0 => None,
            1 => Some(Vec::new()),
            _ => Some(vec![red()]),
        })),
        Job::Ask(Ask::Log(_)) => polled(Answer::Log(
            pick.is_multiple_of(2).then(|| "a line\n".into()),
        )),
    }
}

/// What the runner holds while it interprets a run: the child of each half, and the alarms set.
#[derive(Default)]
struct Runner {
    children: [Option<Job>; 2],
    polls: usize,
    graces: usize,
}

fn slot(half: Half) -> usize {
    match half {
        Half::Tree => 0,
        Half::Live => 1,
    }
}

impl Runner {
    fn take(&mut self, fx: &[Effect]) {
        for effect in fx {
            match effect {
                Effect::Start(half, job) => {
                    assert!(
                        self.children[slot(*half)].is_none(),
                        "one child per half: {fx:?}"
                    );
                    self.children[slot(*half)] = Some(job.clone());
                    let gates = self
                        .children
                        .iter()
                        .filter(|job| is_gate(job.as_ref()))
                        .count();
                    assert!(gates <= 1, "one worktree, so one gate at a time: {fx:?}");
                }
                Effect::Wake(Alarm::Poll) => self.polls += 1,
                Effect::Wake(Alarm::Grace) => self.graces += 1,
                _ => {}
            }
        }
    }

    /// The messages the runner can send now: an end for each child, an alarm set, and, one time
    /// in sixteen, a signal, so most runs go far before they stop.
    fn can_send(&self, which: u8, pick: u8, lost: bool) -> Vec<Msg> {
        let mut msgs: Vec<Msg> = Half::BOTH
            .into_iter()
            .filter_map(|half| {
                Some(end_of(
                    half,
                    self.children[slot(half)].as_ref()?,
                    pick,
                    lost,
                ))
            })
            .collect();
        if self.polls > 0 {
            msgs.push(Msg::Woke(Alarm::Poll));
        }
        if self.graces > 0 {
            msgs.push(Msg::Woke(Alarm::Grace));
        }
        if which.is_multiple_of(16) || msgs.is_empty() {
            msgs.push(Msg::Stop(
                [Signal::HUP, Signal::INT, Signal::TERM][usize::from(pick % 3)],
            ));
        }
        msgs
    }

    fn sent(&mut self, msg: &Msg) {
        match msg {
            Msg::Ended(half, _) => self.children[slot(*half)] = None,
            Msg::Polled(_) => self.children[0] = None,
            Msg::Woke(Alarm::Poll) => self.polls -= 1,
            Msg::Woke(Alarm::Grace) => self.graces -= 1,
            Msg::Stop(_) => {}
        }
    }
}

/// A run of `plan` driven by `choices`, a step at a time, until it is done: each step's message
/// and effects, and the runner's children before it.
struct Trace {
    steps: Vec<(Msg, [Option<Job>; 2], Vec<Effect>)>,
    first: Vec<Effect>,
    done: Done,
}

fn drive(plan: Plan, lost: bool, choices: &[(u8, u8)]) -> Trace {
    let (mut gating, first) = Gating::start(plan);
    let mut runner = Runner::default();
    runner.take(&first);
    let mut steps = Vec::new();
    let mut at = 0;
    loop {
        if let Gating::Done(done) = gating {
            return Trace { steps, first, done };
        }
        let (which, pick) = choices.get(at).copied().unwrap_or((0, 0));
        at += 1;
        let msgs = runner.can_send(which, pick, lost);
        // Past the choices, a stop: a run is always driven to its end.
        let msg = if at > choices.len() {
            msgs.into_iter()
                .find(|m| !matches!(m, Msg::Woke(Alarm::Poll)))
                .expect("a message")
        } else {
            msgs[usize::from(which / 16) % msgs.len()].clone()
        };
        let before = runner.children.clone();
        runner.sent(&msg);
        let (next, fx) = gating::step(gating, msg.clone());
        runner.take(&fx);
        steps.push((msg, before, fx));
        gating = next;
        assert!(steps.len() < 2_000, "the run ends");
    }
}

/// Enough runs that most reach the runners' run beside the live half: each costs microseconds.
const CASES: u32 = 20_000;

fn choices() -> impl Strategy<Value = Vec<(u8, u8)>> {
    prop::collection::vec((any::<u8>(), any::<u8>()), 0..120)
}

/// A plan, whether its runners' run never appears (one run in eight), and the choices.
fn runs() -> impl Strategy<Value = (Plan, bool, Vec<(u8, u8)>)> {
    (
        prop::sample::select(plans()),
        prop::bool::weighted(0.125),
        choices(),
    )
}

fn other(half: Half) -> Half {
    match half {
        Half::Tree => Half::Live,
        Half::Live => Half::Tree,
    }
}

fn is_gate(job: Option<&Job>) -> bool {
    matches!(job, Some(Job::Gate(_)))
}

/// Law 2: after a stop, every running group is signalled in the stop's own step, nothing is
/// written, the run ends stopped once the last child is gone, and the worktree goes after each
/// half's end; after the grace, a group still there gets KILL. After any run, stopped or not, no
/// branch it may have pushed remains: a run whose push started closes it exactly once, one
/// `Cancel` and one `DeleteBranch`, by its commit once the push named it and by its branch before
/// (`check`'s included); only a push its own check refused, which pushed nothing, is not closed.
/// After the nightly lets the lock go, no gate runs and the worktree is not touched.
#[test]
fn law_2_a_stop_signals_every_group_first_and_writes_nothing() {
    proptest!(ProptestConfig::with_cases(CASES), |((plan, lost, choices) in runs())| {
        let trace = drive(plan, lost, &choices);
        let stop = trace.steps.iter().position(|(msg, ..)| matches!(msg, Msg::Stop(_)));
        if let Some(stop) = stop {
            let (_, running, fx) = &trace.steps[stop];
            for half in Half::BOTH {
                if running[slot(half)].is_some() {
                    let signalled = fx.iter().position(|e| *e == Effect::SignalGroup(half, Signal::TERM));
                    prop_assert!(signalled.is_some(), "{half:?} signalled at the stop: {fx:?}");
                    let waits = fx.iter().position(|e| matches!(e, Effect::Cancel(_) | Effect::DeleteBranch(_) | Effect::RemoveWorktree | Effect::Write(..)));
                    prop_assert!(waits.is_none_or(|w| w > signalled.unwrap_or(0)), "signals before anything that waits: {fx:?}");
                }
            }
            for (_, _, fx) in &trace.steps[stop..] {
                prop_assert!(!fx.iter().any(|e| matches!(e, Effect::Write(..) | Effect::Start(..))), "{fx:?}");
            }
            prop_assert!(matches!(trace.done, Done::Stopped(_)), "{:?}", trace.done);
        }
        let mut signalled = [false; 2];
        let mut unlocked = false;
        for (msg, running, fx) in &trace.steps {
            if *msg == Msg::Woke(Alarm::Grace) {
                for half in Half::BOTH {
                    if signalled[slot(half)] && running[slot(half)].is_some() {
                        prop_assert!(fx.contains(&Effect::SignalGroup(half, Signal::KILL)), "KILL after the grace: {fx:?}");
                    }
                }
            }
            for effect in fx {
                match effect {
                    Effect::SignalGroup(half, Signal::TERM) => signalled[slot(*half)] = true,
                    Effect::Unlock => unlocked = true,
                    Effect::Start(_, Job::Gate(_)) | Effect::RemoveWorktree => {
                        prop_assert!(!unlocked, "the worktree after the unlock: {fx:?}");
                    }
                    _ => {}
                }
            }
        }
        let all: Vec<&Effect> = trace.first.iter().chain(trace.steps.iter().flat_map(|(_, _, fx)| fx)).collect();
        let started = all.iter().any(|e| matches!(e, Effect::Start(_, Job::Push(_))));
        let answer = trace.steps.iter().find_map(|(msg, ..)| match msg {
            Msg::Ended(_, Exit::Pushed(answer)) => Some(answer.clone()),
            _ => None,
        });
        let closes = started && !matches!(answer, Some(Err(Check::Leak | Check::Held)));
        let count = |f: fn(&Effect) -> bool| all.iter().filter(|e| f(e)).count();
        prop_assert_eq!(count(|e| matches!(e, Effect::Cancel(_))), usize::from(closes));
        prop_assert_eq!(count(|e| matches!(e, Effect::DeleteBranch(_))), usize::from(closes));
        if closes {
            let by_commit = matches!(answer, Some(Ok(_)));
            prop_assert!(all.iter().any(|e| match e {
                Effect::Cancel(Runs::OfCommit(..)) => by_commit,
                Effect::Cancel(Runs::OfBranch(_)) => !by_commit,
                _ => false,
            }), "closed by commit once the push named it, else by branch: {answer:?}");
        }
        for (msg, running, fx) in &trace.steps {
            let removed = fx.contains(&Effect::RemoveWorktree);
            let gate_ended = matches!(msg, Msg::Ended(half, _) if is_gate(running[slot(*half)].as_ref()));
            prop_assert_eq!(removed, gate_ended, "the worktree goes when, and only when, a gate's group is gone: {:?} {:?}", msg, fx);
        }
    });
}

/// Law 3: where a red half stops the other, a red row comes with the other's group signalled in
/// the same step if it runs, the other starts nothing and writes no green after it, and the red
/// half itself starts nothing more (a red box step pushes nothing; a red box build is its half's
/// red). A half whose group was signalled writes no row. Beside it: no two gates run at once (one
/// worktree), each half writes at most one row, and a done run does nothing.
#[test]
fn law_3_a_red_half_stops_the_other() {
    proptest!(ProptestConfig::with_cases(CASES), |((plan, lost, choices) in runs())| {
        let stops = plan.on_red == OnRed::Stop;
        let trace = drive(plan, lost, &choices);
        let mut red: Option<Half> = None;
        let mut rows = [0, 0];
        let mut signalled = [false; 2];
        for (_, running, fx) in &trace.steps {
            for effect in fx {
                if let Effect::Write(half, _) = effect {
                    rows[slot(*half)] += 1;
                    prop_assert!(!signalled[slot(*half)], "a stopped half wrote a row: {fx:?}");
                }
                if let Effect::SignalGroup(half, Signal::TERM) = effect {
                    signalled[slot(*half)] = true;
                }
                if let (Some(_), Effect::Start(..) | Effect::Write(_, Outcome::Green)) = (red, effect) {
                    prop_assert!(false, "a half went on after a red: {fx:?}");
                }
            }
            if let (true, None, Some(Effect::Write(half, Outcome::Red))) =
                (stops, red, fx.iter().find(|e| matches!(e, Effect::Write(_, Outcome::Red))))
            {
                if running[slot(other(*half))].is_some() {
                    prop_assert!(fx.contains(&Effect::SignalGroup(other(*half), Signal::TERM)), "{fx:?}");
                }
                prop_assert!(!fx.iter().any(|e| matches!(e, Effect::Start(..))), "{fx:?}");
                red = Some(*half);
            }
        }
        prop_assert!(rows.iter().all(|n| *n <= 1), "one row a half: {rows:?}");
        let (after, fx) = gating::step(Gating::Done(trace.done.clone()), Msg::Stop(Signal::TERM));
        prop_assert_eq!(after, Gating::Done(trace.done));
        prop_assert!(fx.is_empty());
    });
}

/// The worst outcome (a watch with nothing pushed, which no verb's plan makes, included): a green
/// row needs every gate piece of its half green (a gate that ended
/// unread, its start failed, is `Unjudged`, so never green) and, for a half on the runners, a run
/// that completed with success and then showed no red job. Every run, unread ends included,
/// reaches its end (`drive` asserts it).
#[test]
fn a_green_row_needs_every_piece_green() {
    proptest!(ProptestConfig::with_cases(CASES), |((plan, lost, choices) in runs())| {
        let watches = plan.tree.iter().any(|step| step.piece == gating::Piece::Watch);
        let trace = drive(plan, lost, &choices);
        let mut not_green = [false; 2];
        let mut succeeded = false;
        let mut completed = false;
        for (msg, _, fx) in &trace.steps {
            match msg {
                Msg::Ended(half, Exit::Gated(ended)) if *ended != Ended::Judged(Outcome::Green) => {
                    not_green[slot(*half)] = true;
                }
                Msg::Polled(Answer::Status(Some(RunStatus::Completed(how)))) => completed = how == "success",
                Msg::Polled(Answer::Jobs(Some(red))) => succeeded = completed && red.is_empty(),
                _ => {}
            }
            for effect in fx {
                if let Effect::Write(half, Outcome::Green) = effect {
                    prop_assert!(!not_green[slot(*half)], "a green row over a piece not green: {fx:?}");
                    if *half == Half::Tree && watches {
                        prop_assert!(succeeded, "a green row with no run of success: {fx:?}");
                    }
                }
            }
        }
    });
}

/// No order of reaping, an error that ends the waits included, yields a green end without a
/// leader's status of 0 read from the kernel: a status nothing read is no status.
#[test]
fn a_green_end_needs_a_leader_status_of_0_read() {
    proptest!(|(answers in prop::collection::vec((any::<bool>(), any::<u8>()), 0..8))| {
        let status = answers.iter().fold(Status::Unread, |status, (leader, code)| status.reaped(*leader, *code));
        let last = answers.iter().rev().find(|(leader, _)| *leader).map(|(_, code)| *code);
        prop_assert_eq!(status, last.map_or(Status::Unread, Status::Read));
        if Ended::of(status) == Ended::Judged(Outcome::Green) {
            prop_assert_eq!(last, Some(0));
        }
    });
    assert_eq!(Ended::of(Status::Unread), Ended::Unjudged);
}

/// The stubbed runs the guards gave up: a run that ends with no red job says nothing; a red job
/// is the tree's red, read off its log, and stops the live half; the live half's early red stops
/// the tree half before its box builds anything, and the branch is closed.
#[test]
fn the_runners_run_and_the_early_live_steps() {
    let start = |plan| {
        let (gating, fx) = Gating::start(plan);
        (gating, fx)
    };
    let step = |gating, msg| gating::step(gating, msg);
    let to_watch = |live: bool| {
        let (g, _) = start(Plan::remote(oid(1), push(), live, OnRed::Stop, 4));
        let (g, _) = step(g, Msg::Ended(Half::Tree, Exit::Checked(Ok(()))));
        let (g, _) = step(
            g,
            Msg::Ended(Half::Tree, Exit::Gated(Ended::Judged(Outcome::Green))),
        );
        let (g, _) = step(g, Msg::Ended(Half::Tree, Exit::Pushed(Ok(oid(2)))));
        let (g, fx) = step(
            g,
            Msg::Ended(Half::Tree, Exit::Dispatched(Dispatch::Started)),
        );
        (g, fx)
    };
    // The live half's early steps red: no publication wait, no box build, no watch.
    let (g, fx) = to_watch(true);
    assert!(
        fx.contains(&Effect::Start(
            Half::Live,
            Job::Gate(gating::Steps::Named(gating::LIVE_EARLY))
        )),
        "{fx:?}"
    );
    let (g, fx) = step(
        g,
        Msg::Ended(Half::Live, Exit::Gated(Ended::Judged(Outcome::Red))),
    );
    assert!(
        fx.contains(&Effect::Write(Half::Live, Outcome::Red))
            && fx.contains(&Effect::Cancel(Runs::OfCommit(oid(2), push().branch))),
        "{fx:?}"
    );
    assert!(
        fx.iter().any(|e| matches!(e, Effect::DeleteBranch(_)))
            && !fx.iter().any(|e| matches!(e, Effect::Start(..))),
        "{fx:?}"
    );
    assert_eq!(
        g,
        Gating::Done(Done::Ran([Ended::Unjudged, Ended::Judged(Outcome::Red)]))
    );

    for (how, red, row) in [
        ("success", false, Some(Outcome::Green)),
        ("cancelled", false, None),
        ("failure", false, None),
        ("failure", true, Some(Outcome::Red)),
    ] {
        let (g, _) = to_watch(false);
        let (g, _) = step(g, Msg::Ended(Half::Tree, Exit::Published("ended".into())));
        let (g, _) = step(
            g,
            Msg::Ended(Half::Tree, Exit::Gated(Ended::Judged(Outcome::Green))),
        );
        let (g, _) = step(g, Msg::Woke(Alarm::Poll));
        let (g, _) = step(g, Msg::Polled(Answer::Run(Some(run()))));
        let (g, _) = step(g, Msg::Woke(Alarm::Poll));
        let (g, _) = step(
            g,
            Msg::Polled(Answer::Status(Some(RunStatus::Completed(how.into())))),
        );
        let jobs = red
            .then(|| RedJob {
                id: 9,
                name: "gate (checks)".into(),
            })
            .into_iter()
            .collect();
        let (mut g, mut fx) = step(g, Msg::Polled(Answer::Jobs(Some(jobs))));
        if red {
            (g, fx) = step(g, Msg::Polled(Answer::Log(Some("clippy FAILED\n".into()))));
            assert!(
                fx.contains(&Effect::AppendLog(
                    "--- job gate (checks)\nclippy FAILED\n".into()
                )),
                "{fx:?}"
            );
        }
        let written: Vec<&Effect> = fx
            .iter()
            .filter(|e| matches!(e, Effect::Write(..)))
            .collect();
        assert_eq!(
            written,
            row.map(|o| Effect::Write(Half::Tree, o))
                .iter()
                .collect::<Vec<_>>(),
            "{how}: {fx:?}"
        );
        assert!(
            fx.contains(&Effect::Cancel(Runs::OfCommit(oid(2), push().branch))),
            "{how}: the run is closed: {fx:?}"
        );
        assert!(matches!(g, Gating::Done(_)), "{how}");
    }
}

/// Law 4: with red monotone over prefixes, the search finds the least red prefix in at most
/// ⌈log₂ n⌉ probes and never probes a prefix the table has judged.
#[test]
fn law_4_bisect_finds_the_least_red_prefix() {
    proptest!(|(n in 1usize..80, red in any::<prop::sample::Index>(), known in prop::collection::vec(any::<bool>(), 80))| {
        struct Table { judged: Vec<Option<Outcome>>, probed: Vec<usize> }
        let red = 1 + red.index(n);
        let truth = |p: usize| if p >= red { Outcome::Red } else { Outcome::Green };
        let mut table = Table {
            judged: (0..=n).map(|p| known[p % 80].then(|| truth(p))).collect(),
            probed: Vec::new(),
        };
        let before = table.judged.clone();
        let found = Search { lo: 0, hi: n }.run(
            &mut table,
            |t, p| Ok::<_, Stuck>(t.judged[p].map_or(Reading::Unjudged, Reading::Agreed)),
            |t, p| { t.probed.push(p); t.judged[p] = Some(truth(p)); Ok(()) },
        ).expect("a monotone table is searched to its end");
        prop_assert_eq!(found, Search { lo: red - 1, hi: red });
        prop_assert!(table.probed.len() <= Search::most(n) as usize, "{:?} over {}", table.probed, n);
        prop_assert!(table.probed.iter().all(|p| before[*p].is_none()), "a judged prefix probed: {:?}", table.probed);
    });
}
