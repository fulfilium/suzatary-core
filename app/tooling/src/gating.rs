//! A gate run as a pure machine: [`step`] takes one message and says what the runner does next.
//! A run's meaning is the rows it writes; a stopped run writes none.

use crate::gate_branch::GateBranch;
use crate::oid::Oid;
use crate::outcome::Check;
use crate::watch::{Answer, Ask, Next, RunRef, Watch};
use crate::{Ended, Half, Outcome};
use rustix::process::Signal;

/// The tree half's steps that read this repository, before the push and after the publication
/// run; the live half's steps that need no publication build, then the rest. The guards in
/// `gate_yml.rs` hold each pair whole.
pub const BOX_BEFORE: &str = "fmt,changelog";
pub const BOX_AFTER: &str = "public-tree,deployed-shape";
pub const LIVE_EARLY: &str = "live,live-all,live-guards";
pub const LIVE_AFTER: &str = "web-live,deploy-build,push-dry,public-check";
pub(crate) const BOX_BUILDS: &str = "the box builds the public tree itself, about 45 minutes of release \
                          build beside the app it serves";

/// Which of a half's gate steps one run of `merge-gate.sh` runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Steps {
    All,
    Named(&'static str),
}

/// A gate branch and the words of its commit (`push-gate`'s arguments).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Push {
    pub branch: GateBranch,
    pub subject: String,
    pub trailer: String,
}

/// One piece of a half.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Piece {
    /// The one check before a push (`train publishable`).
    Check,
    /// `merge-gate.sh` in the worktree.
    Gate(Steps),
    Push(Push),
    /// Master's publication run of the pushed commit, which fills the binary cache.
    Dispatch,
    /// The wait for that run, when it was dispatched.
    Publication,
    /// The lock let go: the nightly's, while only the runners work. The tree half's row written
    /// after it needs no lock: a row is one append of under a hundred bytes, so writers interleave
    /// whole lines, and its key is origin/master's tree, which no train's commit has.
    Unlock,
    /// The run of the pushed commit on GitHub's runners.
    Watch,
}

/// A piece, and the other half's piece that ends before it starts.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Step {
    pub piece: Piece,
    pub after: Option<usize>,
}

/// What a half's red piece does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OnRed {
    /// It ends its half and stops the other; a piece that says nothing ends its half alone.
    Stop,
    /// Every piece runs whatever an earlier one found, red over silent over green (the nightly).
    GoOn,
}

/// What a gate run does, as data.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Plan {
    /// The commit gated, named in what the run says.
    pub commit: Oid,
    pub tree: Vec<Step>,
    pub live: Vec<Step>,
    pub on_red: OnRed,
    /// Polls of the runners' run before the tree half says nothing.
    pub polls: u32,
}

fn now(piece: Piece) -> Step {
    Step { piece, after: None }
}

fn after(piece: Piece, at: usize) -> Step {
    Step {
        piece,
        after: Some(at),
    }
}

impl Plan {
    /// `halves` on the box, the live half after the tree half.
    #[must_use]
    pub fn local(commit: Oid, halves: &[Half], on_red: OnRed) -> Plan {
        let gate = |half, step: Step| {
            if halves.contains(&half) {
                vec![step]
            } else {
                Vec::new()
            }
        };
        Plan {
            commit,
            tree: gate(Half::Tree, now(Piece::Gate(Steps::All))),
            live: gate(Half::Live, after(Piece::Gate(Steps::All), 0)),
            on_red,
            polls: 1,
        }
    }

    /// The tree half on GitHub's runners, and the live half here unless `live` is false (a
    /// bisect's probe). The box has one worktree, so its runs wait for each other; the nightly
    /// lets the lock go before the watch.
    #[must_use]
    pub fn remote(commit: Oid, push: Push, live: bool, on_red: OnRed, polls: u32) -> Plan {
        let mut tree = vec![
            now(Piece::Check),
            now(Piece::Gate(Steps::Named(BOX_BEFORE))),
            now(Piece::Push(push)),
            now(Piece::Dispatch),
            after(Piece::Publication, 0),
            after(Piece::Gate(Steps::Named(BOX_AFTER)), 0),
        ];
        if on_red == OnRed::GoOn {
            tree.push(after(Piece::Unlock, 1));
        }
        tree.push(now(Piece::Watch));
        let live = if live {
            vec![
                after(Piece::Gate(Steps::Named(LIVE_EARLY)), 3),
                after(Piece::Gate(Steps::Named(LIVE_AFTER)), 5),
            ]
        } else {
            Vec::new()
        };
        Plan {
            commit,
            tree,
            live,
            on_red,
            polls,
        }
    }

    /// A pull request's merge on the runners alone (`check`).
    #[must_use]
    pub fn check(commit: Oid, push: Push, polls: u32) -> Plan {
        Plan {
            commit,
            tree: vec![now(Piece::Push(push)), now(Piece::Watch)],
            live: Vec::new(),
            on_red: OnRed::Stop,
            polls,
        }
    }

    fn steps(&self, half: Half) -> &[Step] {
        match half {
            Half::Tree => &self.tree,
            Half::Live => &self.live,
        }
    }
}

/// A child the runner starts for a half.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Job {
    Check,
    Gate(Steps),
    Push(Push),
    Dispatch(Oid),
    Publication(Oid),
    Ask(Ask),
}

/// How a job ended, parsed at the edge from its status and output.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Exit {
    Gated(Ended),
    Checked(Result<(), Check>),
    Pushed(Result<Oid, Check>),
    Dispatched(Dispatch),
    /// The publication run's end, in words: nothing waits on what it says.
    Published(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dispatch {
    Started,
    /// `gh` gave no answer within its bound, and may have started the run.
    Unanswered,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Alarm {
    Poll,
    /// The time a signalled group has before KILL.
    Grace,
}

/// How a job whose status no wait read ends (it could not start, or the kernel reaped it): it
/// says nothing, and no row comes of it.
#[must_use]
pub fn unread(half: Half, job: &Job) -> Msg {
    let exit = match job {
        Job::Gate(_) => Exit::Gated(Ended::Unjudged),
        Job::Check => Exit::Checked(Err(Check::Unrun)),
        Job::Push(_) => Exit::Pushed(Err(Check::Unrun)),
        Job::Dispatch(_) => Exit::Dispatched(Dispatch::Failed),
        Job::Publication(pushed) => Exit::Published(format!(
            "could not ask GitHub for the publication run of {}: {BOX_BUILDS}",
            pushed.short()
        )),
        Job::Ask(ask) => {
            return Msg::Polled(match ask {
                Ask::Find(_) => Answer::Run(None),
                Ask::Status(_) => Answer::Status(None),
                Ask::Jobs(_) => Answer::Jobs(None),
                Ask::Log(_) => Answer::Log(None),
            });
        }
    };
    Msg::Ended(half, exit)
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Msg {
    /// A half's job is gone, every process of its group.
    Ended(Half, Exit),
    /// The tree half's call to GitHub is gone, with its answer.
    Polled(Answer),
    Woke(Alarm),
    Stop(Signal),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Effect {
    Start(Half, Job),
    SignalGroup(Half, Signal),
    Wake(Alarm),
    Write(Half, Outcome),
    AppendLog(String),
    RemoveWorktree,
    /// Every unfinished run of the gate branch, and of its commit's publication, cancelled.
    Cancel(Runs),
    DeleteBranch(GateBranch),
    Unlock,
    /// The runners' run, found.
    Found(RunRef),
    Say(String),
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum At {
    Pending(usize),
    Running(usize),
    Polling(usize, Watch),
    Asking(usize, Watch),
    /// Signalled; its end writes nothing.
    Stopping(usize),
    Ended(Ended),
}

/// The runs a close cancels: the gate branch's, and its commit's publication once the push
/// named the commit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Runs {
    OfBranch(GateBranch),
    OfCommit(Oid, GateBranch),
}

/// The gate branch this run may have pushed: from the moment its push starts, since a push
/// stopped before it answered may already have made the branch on core.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Pushed {
    Nothing,
    Started(GateBranch),
    Open(Oid, GateBranch),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cache {
    Asked,
    NotAsked,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Run {
    plan: Plan,
    at: [At; 2],
    fold: [Ended; 2],
    pushed: Pushed,
    cache: Cache,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Gating {
    Going(Run),
    Stopping(Signal, Run),
    Done(Done),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Done {
    Ran([Ended; 2]),
    Stopped(Signal),
}

fn ix(half: Half) -> usize {
    match half {
        Half::Tree => 0,
        Half::Live => 1,
    }
}

fn other(half: Half) -> Half {
    match half {
        Half::Tree => Half::Live,
        Half::Live => Half::Tree,
    }
}

/// Red over silent over green.
fn worse(a: Ended, b: Ended) -> Ended {
    let rank = |e: Ended| match e {
        Ended::Judged(Outcome::Green) => 0,
        Ended::Unjudged => 1,
        Ended::Judged(Outcome::Red) | Ended::Refused(_) => 2,
    };
    if rank(b) > rank(a) { b } else { a }
}

const GREEN: Ended = Ended::Judged(Outcome::Green);

impl Gating {
    /// A run of `plan`, and the effects that start it.
    #[must_use]
    pub fn start(plan: Plan) -> (Gating, Vec<Effect>) {
        let at = |half| {
            if plan.steps(half).is_empty() {
                At::Ended(Ended::Unjudged)
            } else {
                At::Pending(0)
            }
        };
        let run = Run {
            at: [at(Half::Tree), at(Half::Live)],
            plan,
            fold: [GREEN; 2],
            pushed: Pushed::Nothing,
            cache: Cache::NotAsked,
        };
        settle(Gating::Going(run), Vec::new())
    }
}

/// The machine's one transition.
#[must_use]
pub fn step(gating: Gating, msg: Msg) -> (Gating, Vec<Effect>) {
    let mut fx = Vec::new();
    let gating = match (gating, msg) {
        (done @ Gating::Done(_), _) => return (done, fx),
        (Gating::Going(mut run), Msg::Stop(signal)) => {
            run.stop_all(&mut fx);
            Gating::Stopping(signal, run)
        }
        (stopping @ Gating::Stopping(..), Msg::Stop(_)) => stopping,
        (Gating::Going(mut run), msg) => {
            run.take(msg, &mut fx);
            Gating::Going(run)
        }
        (Gating::Stopping(signal, mut run), msg) => {
            run.take(msg, &mut fx);
            Gating::Stopping(signal, run)
        }
    };
    settle(gating, fx)
}

/// Starts what may start, and ends the run once both halves have ended.
fn settle(gating: Gating, mut fx: Vec<Effect>) -> (Gating, Vec<Effect>) {
    let gating = match gating {
        Gating::Going(mut run) => {
            run.advance(&mut fx);
            match run.ended() {
                Some(ended) => Gating::Done(Done::Ran(ended)),
                None => Gating::Going(run),
            }
        }
        Gating::Stopping(signal, run) => match run.ended() {
            Some(_) => Gating::Done(Done::Stopped(signal)),
            None => Gating::Stopping(signal, run),
        },
        done @ Gating::Done(_) => done,
    };
    (gating, fx)
}

impl Run {
    fn at(&mut self, half: Half) -> &mut At {
        &mut self.at[ix(half)]
    }

    fn ended(&self) -> Option<[Ended; 2]> {
        match &self.at {
            [At::Ended(tree), At::Ended(live)] => Some([*tree, *live]),
            _ => None,
        }
    }

    fn short(&self) -> String {
        self.plan.commit.short().to_owned()
    }

    fn piece(&self, half: Half, at: usize) -> &Piece {
        &self.plan.steps(half)[at].piece
    }

    fn take(&mut self, msg: Msg, fx: &mut Vec<Effect>) {
        match msg {
            Msg::Stop(_) => {}
            Msg::Woke(Alarm::Grace) => {
                for half in Half::BOTH {
                    if let At::Stopping(_) = self.at(half) {
                        fx.push(Effect::SignalGroup(half, Signal::KILL));
                    }
                }
            }
            Msg::Woke(Alarm::Poll) => {
                if let At::Polling(at, watch) = self.at(Half::Tree).clone()
                    && let Pushed::Open(pushed, _) = &self.pushed
                {
                    fx.push(Effect::Start(Half::Tree, Job::Ask(watch.ask(pushed))));
                    *self.at(Half::Tree) = At::Asking(at, watch);
                }
            }
            Msg::Polled(answer) => match self.at(Half::Tree).clone() {
                At::Asking(at, watch) => {
                    let (limit, short) = (self.plan.polls, self.short());
                    match watch.answer(answer, limit, &short, fx) {
                        Next::Wait(watch) => {
                            *self.at(Half::Tree) = At::Polling(at, watch);
                            fx.push(Effect::Wake(Alarm::Poll));
                        }
                        Next::Ask(watch) => {
                            *self.at(Half::Tree) = At::Polling(at, watch);
                            self.take(Msg::Woke(Alarm::Poll), fx);
                        }
                        Next::Ended(ended) => self.piece_ended(Half::Tree, at, ended, fx),
                    }
                }
                At::Stopping(at) => self.stopped(Half::Tree, at, fx),
                _ => {}
            },
            Msg::Ended(half, exit) => match self.at(half).clone() {
                At::Running(at) => self.finish(half, at, exit, fx),
                At::Stopping(at) => {
                    // A push that answered after the stop names its commit for the close.
                    match (exit, self.piece(half, at)) {
                        (Exit::Pushed(Ok(pushed)), Piece::Push(push)) => {
                            self.pushed = Pushed::Open(pushed, push.branch.clone());
                        }
                        (Exit::Pushed(Err(check)), _) => self.refused(check),
                        _ => {}
                    }
                    self.stopped(half, at, fx);
                }
                _ => {}
            },
        }
    }

    /// A push its own check refused (a leak, a held admission) pushed nothing, so there is no
    /// branch to close; one that could not run may have, and stays to be closed by its branch.
    fn refused(&mut self, check: Check) {
        if let Check::Leak | Check::Held = check {
            self.pushed = Pushed::Nothing;
        }
    }

    /// The half's job is gone after a signal: nothing it found is written.
    fn stopped(&mut self, half: Half, at: usize, fx: &mut Vec<Effect>) {
        if let Piece::Gate(_) = self.piece(half, at) {
            fx.push(Effect::RemoveWorktree);
        }
        self.end(half, Ended::Unjudged, fx);
    }

    fn finish(&mut self, half: Half, at: usize, exit: Exit, fx: &mut Vec<Effect>) {
        let short = self.short();
        let piece = self.piece(half, at).clone();
        if let Piece::Gate(_) = piece {
            fx.push(Effect::RemoveWorktree);
        }
        let kept = |fx: &mut Vec<Effect>, check, then: &str| {
            fx.push(Effect::Say(format!(
                "nothing went to core, so the tree half says nothing of {short}{then}"
            )));
            Ended::Refused(check)
        };
        let ended = match (exit, &piece) {
            (Exit::Gated(ended), _) => ended,
            (Exit::Checked(Err(check)), _) => {
                kept(fx, check, "; the live half's public-check judges it")
            }
            (Exit::Pushed(Err(check)), _) => {
                self.refused(check);
                kept(fx, check, "")
            }
            (Exit::Pushed(Ok(pushed)), Piece::Push(push)) => {
                fx.push(Effect::Say(format!(
                    "the tree half of {short} runs on GitHub's runners: core, branch {}, commit {}",
                    push.branch,
                    pushed.short()
                )));
                self.pushed = Pushed::Open(pushed, push.branch.clone());
                GREEN
            }
            // A push's end comes only for a push piece (`Supervisor` starts one job per piece).
            (Exit::Checked(Ok(())) | Exit::Pushed(Ok(_)), _) => GREEN,
            (Exit::Dispatched(dispatch), _) => {
                self.cache = Cache::Asked;
                match dispatch {
                    Dispatch::Started => {}
                    Dispatch::Unanswered => fx.push(Effect::Say(format!(
                        "the dispatch of the publication run of {short} got no answer within \
                         60s: looking for the run all the same"
                    ))),
                    Dispatch::Failed => {
                        self.cache = Cache::NotAsked;
                        fx.push(Effect::Say(format!(
                            "could not start the publication run of {short} (above): {BOX_BUILDS}"
                        )));
                    }
                }
                GREEN
            }
            (Exit::Published(said), _) => {
                fx.push(Effect::Say(said));
                GREEN
            }
        };
        self.piece_ended(half, at, ended, fx);
    }

    fn piece_ended(&mut self, half: Half, at: usize, ended: Ended, fx: &mut Vec<Effect>) {
        let fold = worse(self.fold[ix(half)], ended);
        self.fold[ix(half)] = fold;
        match (ended, self.plan.on_red) {
            (Ended::Refused(_), _) => self.end(half, ended, fx),
            (GREEN, _) | (_, OnRed::GoOn) => *self.at(half) = At::Pending(at + 1),
            (_, OnRed::Stop) => self.end(half, ended, fx),
        }
    }

    /// The half ends: its row, and the gate branch closed once the tree half needs it no more.
    fn end(&mut self, half: Half, ended: Ended, fx: &mut Vec<Effect>) {
        *self.at(half) = At::Ended(ended);
        if let Ended::Judged(outcome) = ended {
            fx.push(Effect::Write(half, outcome));
        }
        if half == Half::Tree {
            let (runs, branch) = match std::mem::replace(&mut self.pushed, Pushed::Nothing) {
                Pushed::Nothing => (None, None),
                Pushed::Started(branch) => (Some(Runs::OfBranch(branch.clone())), Some(branch)),
                Pushed::Open(pushed, branch) => {
                    (Some(Runs::OfCommit(pushed, branch.clone())), Some(branch))
                }
            };
            fx.extend(runs.map(Effect::Cancel));
            fx.extend(branch.map(Effect::DeleteBranch));
        }
        if ended == Ended::Judged(Outcome::Red) && self.plan.on_red == OnRed::Stop {
            self.stop_other(half, fx);
        }
    }

    fn stop_other(&mut self, red: Half, fx: &mut Vec<Effect>) {
        let half = other(red);
        let (red_name, name) = (red.name(), half.name());
        match self.at(half).clone() {
            At::Running(at) | At::Asking(at, _) => {
                fx.push(Effect::Say(format!(
                    "the {red_name} half is red, so the {name} half is stopped"
                )));
                fx.push(Effect::SignalGroup(half, Signal::TERM));
                fx.push(Effect::Wake(Alarm::Grace));
                *self.at(half) = At::Stopping(at);
            }
            At::Pending(_) | At::Polling(..) => {
                fx.push(Effect::Say(format!(
                    "the {red_name} half is red, so the {name} half is stopped"
                )));
                self.end(half, Ended::Unjudged, fx);
            }
            At::Stopping(_) | At::Ended(_) => {}
        }
    }

    /// Every group still running signalled in this one step, before anything that waits (one a
    /// red already stopped, again); then the halves with no child end.
    fn stop_all(&mut self, fx: &mut Vec<Effect>) {
        let mut signalled = false;
        for half in Half::BOTH {
            if let At::Running(at) | At::Asking(at, _) | At::Stopping(at) = self.at(half).clone() {
                fx.push(Effect::SignalGroup(half, Signal::TERM));
                *self.at(half) = At::Stopping(at);
                signalled = true;
            }
        }
        if signalled {
            fx.push(Effect::Wake(Alarm::Grace));
        }
        for half in Half::BOTH {
            if let At::Pending(_) | At::Polling(..) = self.at(half) {
                self.end(half, Ended::Unjudged, fx);
            }
        }
    }

    /// Starts each half's next piece whose wait is met, until none can.
    fn advance(&mut self, fx: &mut Vec<Effect>) {
        let mut moved = true;
        while moved {
            moved = false;
            for half in Half::BOTH {
                let At::Pending(at) = *self.at(half) else {
                    continue;
                };
                match self.plan.steps(half).get(at).cloned() {
                    None => self.end(half, self.fold[ix(half)], fx),
                    Some(step) if self.passed(other(half), step.after) => {
                        self.begin(half, at, step.piece, fx);
                    }
                    Some(_) => continue,
                }
                moved = true;
            }
        }
    }

    /// Whether `half` has ended its piece `after`.
    fn passed(&self, half: Half, after: Option<usize>) -> bool {
        after.is_none_or(|piece| match &self.at[ix(half)] {
            At::Pending(at)
            | At::Running(at)
            | At::Polling(at, _)
            | At::Asking(at, _)
            | At::Stopping(at) => *at > piece,
            At::Ended(_) => true,
        })
    }

    fn begin(&mut self, half: Half, at: usize, piece: Piece, fx: &mut Vec<Effect>) {
        let pushed = match &self.pushed {
            Pushed::Open(pushed, _) => Some(pushed.clone()),
            Pushed::Nothing | Pushed::Started(_) => None,
        };
        let job = match (piece, pushed, self.cache) {
            (Piece::Check, ..) => Job::Check,
            (Piece::Gate(steps), ..) => Job::Gate(steps),
            (Piece::Push(push), ..) => {
                self.pushed = Pushed::Started(push.branch.clone());
                Job::Push(push)
            }
            (Piece::Dispatch, Some(pushed), _) => Job::Dispatch(pushed),
            (Piece::Publication, Some(pushed), Cache::Asked) => {
                fx.push(Effect::Say(format!(
                    "waiting for the publication run of {} before the box builds the public tree",
                    pushed.short()
                )));
                Job::Publication(pushed)
            }
            (Piece::Watch, Some(_), _) => {
                *self.at(half) = At::Polling(at, Watch::START);
                fx.push(Effect::Wake(Alarm::Poll));
                return;
            }
            (Piece::Unlock, ..) => {
                fx.push(Effect::Unlock);
                *self.at(half) = At::Pending(at + 1);
                return;
            }
            // A watch with nothing pushed has no run to watch: the half says nothing of the tree,
            // rather than a fold that ends green with no run read.
            (Piece::Watch, None, _) => {
                self.piece_ended(half, at, Ended::Unjudged, fx);
                return;
            }
            // Nothing pushed, or no publication run asked for: the piece has nothing to do.
            (Piece::Dispatch | Piece::Publication, ..) => {
                *self.at(half) = At::Pending(at + 1);
                return;
            }
        };
        fx.push(Effect::Start(half, job));
        *self.at(half) = At::Running(at);
    }
}
