//! The edge of a gate run: each `Effect` of [`gating::step`] done, and each child's end parsed
//! into its message. The verbs that run plans are in `verbs.rs`.

use crate::data::{Beside, Data};
use crate::gate_branch::GateBranch;
use crate::gating::{self, Alarm, Dispatch, Done, Effect, Exit, Gating, Job, Msg, OnRed};
use crate::gating::{Plan, Push, Runs, Steps};
use crate::lock::{Holder, Lock};
use crate::oid::Oid;
use crate::outcome::Check;
use crate::supervisor::{Capture, Child, Event, Status, Supervisor};
use crate::watch::{Answer, Ask, RedJob, RunRef, RunStatus};
use crate::{Ended, Error, Half, Reading, Row, Scope, Seen, Table, git, process};
use rustix::process::Signal;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// The train's own test seams and signing keys, and the variables it hands its own children: a
/// gate is handed none.
pub(crate) const TRAIN_READS: [&str; 13] = [
    "SUZATARY_TRAIN_BIN",
    "SUZATARY_TRAIN_GATE",
    "SUZATARY_TRAIN_GROWTH",
    "SUZATARY_TRAIN_GH",
    "SUZATARY_TRAIN_POLL",
    "SUZATARY_TRAIN_PIN",
    "SUZATARY_TRAIN_PUBLIC",
    "SUZATARY_TRAIN_CACHE_TIMEOUT",
    "SUZATARY_BOX_KEY",
    "SUZATARY_AGENT_KEY",
    "SUZATARY_DEDUP_KEY",
    HOLDER,
    SCRIPTS,
];
/// The directory of the `train.sh` that ran the tool, whose scripts the train runs.
const SCRIPTS: &str = "SUZATARY_SCRIPTS";
/// Set in what the train runs under its lock, so `train.sh` knows it holds it.
pub(crate) const HOLDER: &str = "SUZATARY_LOCK_HOLDER";
/// A gate run not ended in six hours (GitHub's job limit) says nothing.
const RUN_SECONDS: u64 = 6 * 3600;

/// What the growth check found of the commits since the merge base.
pub(crate) enum Growth {
    Within,
    Refused(Vec<String>),
    /// It compared nothing, and says why: never a pass.
    Unrun(String),
}

/// Who a child is: a half's job, or a command the train runs on its own behalf.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Key {
    Job(Half, Job),
    Attended,
}

/// The gates' flag, and the rows they write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kept {
    Selective,
    Full,
    Nightly,
}

/// Whether a run writes rows: a gate's, at the data read when it began, or none, for a `check`,
/// which runs no gate here, and a train that began none.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Depth {
    Kept(Kept, Data),
    Unkept,
}

/// The guards' seams (`seams`), else the deployed train's own.
pub(crate) struct Seams {
    pub(crate) gate: Option<PathBuf>,
    /// A program in the growth check's place: exit 0 within, 1 refused (its lines on stdout).
    pub(crate) growth: Option<PathBuf>,
    pub(crate) gh: OsString,
    pub(crate) poll: Duration,
    pub(crate) cache_timeout: u64,
}

fn seams() -> Seams {
    #[cfg(feature = "seams")]
    {
        let var = |name| std::env::var_os(name).filter(|v| !v.is_empty());
        let number = |name, or| {
            var(name)
                .and_then(|v| v.to_str()?.parse().ok())
                .unwrap_or(or)
        };
        Seams {
            gate: var("SUZATARY_TRAIN_GATE").map(PathBuf::from),
            growth: var("SUZATARY_TRAIN_GROWTH").map(PathBuf::from),
            gh: var("SUZATARY_TRAIN_GH").unwrap_or_else(|| "gh".into()),
            poll: Duration::from_secs(number("SUZATARY_TRAIN_POLL", 60)),
            cache_timeout: number("SUZATARY_TRAIN_CACHE_TIMEOUT", 12_000),
        }
    }
    #[cfg(not(feature = "seams"))]
    Seams {
        gate: None,
        growth: None,
        gh: "gh".into(),
        poll: Duration::from_mins(1),
        cache_timeout: 12_000,
    }
}

/// The train in this checkout: the runner and every resource a run owns.
pub struct Train {
    pub(crate) root: PathBuf,
    pub(crate) scripts: PathBuf,
    pub(crate) state: PathBuf,
    pub(crate) sup: Supervisor<Key, Alarm>,
    pub(crate) seams: Seams,
    table: Table,
    /// The repository `core` names on GitHub, `owner/name`.
    pub(crate) repo: Option<String>,
    pub(crate) instance: Option<PathBuf>,
    pub(crate) lock: Option<Lock>,
    pub(crate) depth: Depth,
    pub(crate) logs: Logs,
}

/// Where the tree half's log is: the tree's, or a `check`'s own, which no gate's log shares.
pub(crate) enum Logs {
    ByTree,
    Check(PathBuf),
}

/// How a run ended, and the runners' run it found.
pub(crate) struct Ran {
    pub(crate) ended: [Ended; 2],
    pub(crate) run: Option<RunRef>,
}

pub(crate) fn said(text: impl Into<String>) -> Error {
    Error::Said(text.into())
}

impl Train {
    pub fn new(table: Table) -> Result<Train, Error> {
        let root = git::top()?;
        let url = git::out(&["config", "--get", "remote.core.url"]).ok();
        Ok(Train {
            scripts: std::env::var_os(SCRIPTS).map_or_else(|| root.join("scripts"), PathBuf::from),
            state: git::common_dir()?.join("train"),
            sup: Supervisor::new().map_err(|e| Error::Unrun(format!("the runner: {e}")))?,
            seams: seams(),
            table,
            repo: url.as_deref().and_then(github_repo),
            instance: None,
            lock: None,
            depth: Depth::Unkept,
            logs: Logs::ByTree,
            root,
        })
    }

    pub(crate) fn holder<'a>(&'a self, verb: &'a str) -> Holder<'a> {
        Holder {
            state: &self.state,
            verb,
        }
    }

    pub(crate) fn log(&self, half: Half, commit: &Oid) -> Result<PathBuf, Error> {
        let logs = self.state.join("logs");
        std::fs::create_dir_all(&logs).map_err(|source| Error::Io {
            path: logs.clone(),
            source,
        })?;
        let tree = git::tree(&commit.to_string())?;
        Ok(logs.join(format!("{}-{tree}.log", half.name())))
    }

    /// Every gate's rows, as a land reads them; the nightly asks of full gates' alone.
    pub(crate) fn reading(&self, half: Half, commit: &Oid) -> Result<Reading, Error> {
        let seen = match &self.depth {
            Depth::Kept(Kept::Nightly, _) => Seen::Full,
            Depth::Kept(Kept::Selective | Kept::Full, _) | Depth::Unkept => Seen::Any,
        };
        self.table
            .read(half, seen, &git::tree(&commit.to_string())?)
    }

    /// A live red judged at other stores than this run's, said before its probes run here.
    pub(crate) fn say_moved_stores(&self, commit: &Oid) -> Result<(), Error> {
        let tree = git::tree(&commit.to_string())?;
        let key = self.table.judged(Half::Live, Seen::Any, &tree)?.key;
        if let (Some(Beside::Data(red)), Depth::Kept(_, now)) = (key, &self.depth)
            && red != *now
        {
            eprintln!(
                "train: the live half's red was judged at the stores {}; the probes run at the \
                 stores as they stand, {}, so each prefix's verdict is of those",
                red.short(),
                now.short()
            );
        }
        Ok(())
    }

    pub(crate) fn repo(&self) -> Result<&str, Error> {
        self.repo.as_deref().ok_or_else(|| {
            said(
                "the remote `core` is not a GitHub repository, so the tree half cannot run on \
                 GitHub's runners.",
            )
        })
    }

    /// `halves` of `commit`, the tree half on GitHub's runners where `prefix` names its branch.
    pub(crate) fn plan(
        &self,
        commit: &Oid,
        halves: &[Half],
        remote: Option<&str>,
        on_red: OnRed,
    ) -> Result<Plan, Error> {
        let Some(prefix) = remote.filter(|_| halves.contains(&Half::Tree)) else {
            return Ok(Plan::local(commit.clone(), halves, on_red));
        };
        let push = Push {
            branch: GateBranch::parse(&format!("{prefix}{commit}"))?,
            subject: format!("Gate {}", commit.short()),
            trailer: format!("Gate: {commit}"),
        };
        let live = halves.contains(&Half::Live);
        Ok(Plan::remote(
            commit.clone(),
            push,
            live,
            on_red,
            self.polls(),
        ))
    }

    pub(crate) fn polls(&self) -> u32 {
        u32::try_from(RUN_SECONDS / self.seams.poll.as_secs().max(1)).unwrap_or(u32::MAX)
    }

    /// `cmd` in the train's own process group; a signal is passed on to it, and the train ends,
    /// stopped, once it has.
    pub(crate) fn attend(&mut self, cmd: Command) -> Result<u8, Error> {
        let child = self.sup.start(Key::Attended, cmd, Capture::Own);
        let mut stop = None;
        loop {
            match self.sup.recv() {
                Event::Signal(signal) => {
                    child.inspect(|child| child.signal(signal));
                    stop.get_or_insert(signal);
                }
                Event::Exited {
                    key: Key::Attended,
                    status,
                    ..
                } => {
                    return match (stop, status) {
                        (Some(signal), _) => Err(Error::Stopped(signal.as_raw())),
                        (None, Status::Read(code)) => Ok(code),
                        (None, Status::Unread) => Err(Error::Unrun(
                            "no wait read the command's exit status".into(),
                        )),
                    };
                }
                Event::Exited { .. } | Event::Woke(_) => {}
            }
        }
    }

    /// Runs `plan` to its end. An effect that fails stops the run as a signal would, so no
    /// half outlives it, and the run's error is that effect's.
    pub(crate) fn run(&mut self, plan: Plan) -> Result<Ran, Error> {
        let commit = plan.commit.clone();
        let mut children: [Option<Child>; 2] = [None, None];
        let (mut found, mut failed) = (None, None);
        let (mut gating, mut fx) = Gating::start(plan);
        loop {
            for effect in std::mem::take(&mut fx) {
                if let Err(e) = self.effect(effect, &commit, &mut children, &mut found) {
                    failed.get_or_insert(e);
                }
            }
            let msg = match (gating, failed.is_some()) {
                (Gating::Done(done), _) => {
                    return match (done, failed) {
                        (_, Some(e)) => Err(e),
                        (Done::Ran(ended), None) => Ok(Ran { ended, run: found }),
                        (Done::Stopped(signal), None) => Err(Error::Stopped(signal.as_raw())),
                    };
                }
                (going @ Gating::Going(_), true) => {
                    gating = going;
                    Msg::Stop(Signal::TERM)
                }
                (going, _) => {
                    gating = going;
                    match self.sup.recv() {
                        Event::Exited {
                            key: Key::Job(half, job),
                            status,
                            out,
                        } => {
                            children[slot(half)] = None;
                            self.message(half, &job, status, &out)
                        }
                        Event::Exited { .. } => continue,
                        Event::Signal(signal) => Msg::Stop(signal),
                        Event::Woke(alarm) => Msg::Woke(alarm),
                    }
                }
            };
            (gating, fx) = gating::step(gating, msg);
        }
    }

    fn effect(
        &mut self,
        effect: Effect,
        commit: &Oid,
        children: &mut [Option<Child>; 2],
        found: &mut Option<RunRef>,
    ) -> Result<(), Error> {
        let kept = match &self.depth {
            Depth::Kept(Kept::Selective, data) => Some((Scope::Selective, data.clone())),
            Depth::Kept(Kept::Full | Kept::Nightly, data) => Some((Scope::Full, data.clone())),
            Depth::Unkept => None,
        };
        match (effect, kept) {
            // A piece that could not start has ended, its status unread: the run reaches `Done`.
            (Effect::Start(half, job), _) => match self.start(half, job.clone(), commit) {
                Ok(child) => children[slot(half)] = child,
                Err(e) => {
                    let (key, status, out) = (Key::Job(half, job), Status::Unread, String::new());
                    self.sup.post(Event::Exited { key, status, out });
                    return Err(e);
                }
            },
            (Effect::SignalGroup(half, signal), _) => {
                children[slot(half)].inspect(|child| child.signal(signal));
            }
            (Effect::Wake(alarm), _) => {
                let after = match alarm {
                    Alarm::Poll => self.seams.poll,
                    Alarm::Grace => crate::supervisor::GRACE,
                };
                self.sup.wake(after, alarm);
            }
            (Effect::Write(half, outcome), Some((scope, data))) => {
                let row = Row {
                    outcome,
                    commit: commit.clone(),
                    at: crate::instant::Instant::now(),
                    beside: Beside::of(half, &data),
                };
                let tree = git::tree(&commit.to_string())?;
                self.table.write(half, scope, &tree, &row)?;
            }
            (Effect::Write(..), None) => {}
            (Effect::AppendLog(text), _) => {
                let path = match &self.logs {
                    Logs::Check(path) => path.clone(),
                    Logs::ByTree => self.log(Half::Tree, commit)?,
                };
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut log| std::io::Write::write_all(&mut log, text.as_bytes()))
                    .map_err(|source| Error::Io { path, source })?;
            }
            (Effect::RemoveWorktree, _) => self.worktree_close(),
            (Effect::Cancel(runs), _) => self.cancel(&runs),
            (Effect::DeleteBranch(branch), _) => delete(&branch),
            (Effect::Unlock, _) => self.lock = None,
            (Effect::Found(run), _) => *found = Some(run),
            (Effect::Say(words), _) => eprintln!("train: {words}"),
        }
        Ok(())
    }

    fn start(&mut self, half: Half, job: Job, commit: &Oid) -> Result<Option<Child>, Error> {
        let tool = |args: &[&str]| -> Result<Command, Error> {
            let exe = std::env::current_exe().map_err(|e| Error::Unrun(e.to_string()))?;
            let mut cmd = Command::new(exe);
            cmd.args(args);
            Ok(cmd)
        };
        let (cmd, capture) = match &job {
            Job::Gate(steps) if !self.worktree_open(commit) => {
                eprintln!("train: no worktree could be made at {}", commit.short());
                let (key, status, out) = (Key::Job(half, job), Status::Unread, String::new());
                self.sup.post(Event::Exited { key, status, out });
                return Ok(None);
            }
            Job::Gate(steps) => (
                self.merge_gate(half, *steps),
                Capture::Tee(self.log(half, commit)?),
            ),
            Job::Check => (tool(&["publishable", &commit.to_string()])?, Capture::Out),
            Job::Push(Push {
                branch,
                subject,
                trailer,
            }) => {
                let (commit, branch) = (commit.to_string(), branch.to_string());
                let args = ["push-gate", &commit, &branch, subject, trailer];
                (tool(&args)?, Capture::Out)
            }
            Job::Dispatch(pushed) => {
                let mut cmd = self.gh();
                cmd.args(["workflow", "run", "publication.yml", "--repo", self.repo()?])
                    .args(["--ref", "master", "-f", &format!("commit={pushed}")]);
                (cmd, Capture::Out)
            }
            Job::Publication(pushed) => {
                let mut cmd = Command::new(self.scripts.join("publication-run.sh"));
                let times = [self.seams.cache_timeout, self.seams.poll.as_secs()];
                cmd.arg(&self.seams.gh)
                    .args([
                        self.repo()?,
                        "workflow_dispatch",
                        &pushed.to_string(),
                        "600",
                    ])
                    .args(times.map(|n| n.to_string()));
                (cmd, Capture::Out)
            }
            Job::Ask(ask) => (self.ask(ask)?, Capture::Out),
        };
        let child = self.sup.start(Key::Job(half, job.clone()), cmd, capture);
        if let (Job::Gate(_), Some(Child::Group(pid))) = (&job, child) {
            let (name, group) = (half.name(), pid.as_raw_nonzero());
            eprintln!("train: the {name} half runs as process group {group}");
        }
        Ok(child)
    }

    /// The merge gate in the worktree, as `half`, with none of what the train reads for itself.
    fn merge_gate(&self, half: Half, steps: Steps) -> Command {
        let worktree = self.state.join("worktree");
        let gate = self.seams.gate.clone();
        let mut cmd = Command::new(gate.unwrap_or_else(|| worktree.join("scripts/merge-gate.sh")));
        cmd.args(match &self.depth {
            Depth::Kept(Kept::Full, _) => &["--full"][..],
            Depth::Kept(Kept::Nightly, _) => &["--nightly"],
            Depth::Kept(Kept::Selective, _) | Depth::Unkept => &[],
        });
        cmd.arg(match half {
            Half::Tree => "--no-live",
            Half::Live => "--only-live",
        });
        if let Steps::Named(steps) = steps {
            cmd.args(["--steps", steps]);
        }
        for name in TRAIN_READS {
            cmd.env_remove(name);
        }
        if let Some(instance) = &self.instance {
            cmd.env("SUZATARY_INSTANCE", instance);
        }
        // In every live step's pass key, so a pass is kept for the data it was judged at.
        if let (Half::Live, Depth::Kept(_, data)) = (half, &self.depth) {
            cmd.env("SUZATARY_GATE_DATA", data.to_string());
        }
        cmd.env("SUZATARY_GATE_EVIDENCE", self.state.join("logs/evidence"));
        cmd.current_dir(worktree);
        cmd
    }

    /// What the commits since the merge base declare of the growth of `commit` (`context::growth`),
    /// read in its worktree.
    pub(crate) fn history(&mut self, commit: &Oid) -> Growth {
        if !self.worktree_open(commit) {
            return Growth::Unrun(format!("no worktree could be made at {}", commit.short()));
        }
        let worktree = self.state.join("worktree");
        let found = match &self.seams.growth {
            Some(stub) => {
                let mut cmd = Command::new(stub);
                cmd.current_dir(&worktree);
                match process::output(&mut cmd) {
                    Ok(out) if out.status.code() == Some(1) => {
                        Ok(String::from_utf8_lossy(&out.stdout)
                            .lines()
                            .map(str::to_owned)
                            .collect())
                    }
                    Ok(out) if out.status.success() => Ok(Vec::new()),
                    Ok(out) => Err(anyhow::anyhow!(
                        "{}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    )),
                    Err(e) => Err(anyhow::anyhow!("{e}")),
                }
            }
            None => crate::context::growth(&worktree),
        };
        self.worktree_close();
        match found {
            Ok(refused) if refused.is_empty() => Growth::Within,
            Ok(refused) => Growth::Refused(refused),
            Err(e) => Growth::Unrun(format!("{e:#}")),
        }
    }

    /// `gh`, bounded by 60 s, as every call the train makes to GitHub.
    pub(crate) fn gh(&self) -> Command {
        let mut cmd = Command::new("timeout");
        cmd.arg("60").arg(&self.seams.gh);
        cmd
    }

    fn ask(&self, ask: &Ask) -> Result<Command, Error> {
        let repo = self.repo()?;
        let runs = format!("repos/{repo}/actions/runs");
        let (path, jq) = match ask {
            Ask::Find(pushed) => (
                format!("{runs}?head_sha={pushed}&event=push"),
                r#".workflow_runs[] | select(.path == ".github/workflows/gate.yml") | .id"#,
            ),
            Ask::Status(run) => (
                format!("{runs}/{}", run.id),
                r#".status + " " + (.conclusion // "")"#,
            ),
            Ask::Jobs(run) => (
                format!("{runs}/{}/jobs?per_page=100", run.id),
                r#".jobs[] | select(any((.steps // [])[]; .name == "red" and .conclusion == "failure")) | "\(.id) \(.name)""#,
            ),
            Ask::Log(job) => (format!("repos/{repo}/actions/jobs/{}/logs", job.id), ""),
        };
        let mut cmd = self.gh();
        cmd.args(["api", &path]);
        if !jq.is_empty() {
            cmd.args(["--jq", jq]);
        }
        Ok(cmd)
    }

    /// A job's end as the message its piece reads.
    fn message(&self, half: Half, job: &Job, status: Status, out: &str) -> Msg {
        if status == Status::Unread {
            return gating::unread(half, job);
        }
        let ok = status == Status::Read(0);
        let lines = || out.lines().filter(|line| !line.is_empty());
        let exit = match job {
            Job::Gate(_) => Exit::Gated(Ended::of(status)),
            Job::Check => Exit::Checked(Check::of(status)),
            Job::Push(_) => Exit::Pushed(
                Check::of(status).and_then(|()| Oid::parse(out.trim()).map_err(|_| Check::Unrun)),
            ),
            Job::Dispatch(_) => Exit::Dispatched(match status {
                Status::Read(0) => Dispatch::Started,
                Status::Read(124) => Dispatch::Unanswered,
                Status::Read(_) | Status::Unread => Dispatch::Failed,
            }),
            Job::Publication(pushed) => Exit::Published(self.published(pushed, out)),
            Job::Ask(Ask::Find(_)) => {
                let repo = self.repo.as_deref().unwrap_or_default();
                let id = lines()
                    .next()
                    .filter(|_| ok)
                    .and_then(|id| id.parse::<u64>().ok());
                return Msg::Polled(Answer::Run(id.map(|id| RunRef {
                    id,
                    url: format!("https://github.com/{repo}/actions/runs/{id}"),
                })));
            }
            Job::Ask(Ask::Status(_)) => {
                return Msg::Polled(Answer::Status(ok.then(
                    || match out.trim_end().split_once(' ') {
                        Some(("completed", how)) => RunStatus::Completed(how.to_owned()),
                        _ => RunStatus::Going(out.trim().to_owned()),
                    },
                )));
            }
            Job::Ask(Ask::Jobs(_)) => {
                // A job's name is the workflow's, a pull request's under `check`: only its id is
                // read as a number, and a line whose id is none fails the answer.
                let job = |line: &str| {
                    let (id, name) = line.split_once(' ').unwrap_or((line, ""));
                    let name = name.to_owned();
                    id.parse().ok().map(|id| RedJob { id, name })
                };
                let jobs = lines().map(job).collect::<Option<Vec<_>>>();
                return Msg::Polled(Answer::Jobs(jobs.filter(|_| ok)));
            }
            Job::Ask(Ask::Log(_)) => {
                let log = out.lines().flat_map(|line| [untimed(line), "\n"]).collect();
                return Msg::Polled(Answer::Log(ok.then_some(log)));
            }
        };
        Msg::Ended(half, exit)
    }

    /// `publication-run.sh`'s word, in the train's words.
    fn published(&self, pushed: &Oid, out: &str) -> String {
        let short = pushed.short();
        let builds = gating::BOX_BUILDS;
        let of = format!("the publication run of {short}");
        match out.split_whitespace().collect::<Vec<_>>()[..] {
            ["success", ..] => {
                format!("{of} succeeded: the box's builds come from the binary cache")
            }
            ["failed", url, how, ..] => format!("{of} ended {how} ({url}): {builds}"),
            ["running", url, how, ..] => {
                let after = self.seams.cache_timeout;
                format!("{of} was still {how} after {after}s ({url}): {builds}")
            }
            ["none", ..] => format!("no publication run of {short} appeared within 600s: {builds}"),
            _ => format!("could not ask GitHub for {of} (gh's words above): {builds}"),
        }
    }

    /// One worktree path, so cargo's fingerprints, which name it, hold; the target moves in.
    fn worktree_open(&self, commit: &Oid) -> bool {
        self.worktree_close();
        let (worktree, target) = (self.state.join("worktree"), self.state.join("target"));
        let commit = commit.to_string();
        let add = [OsStr::new("worktree"), "add".as_ref(), "--quiet".as_ref()];
        let add = [
            &add[..],
            &["--detach".as_ref(), worktree.as_os_str(), commit.as_ref()],
        ]
        .concat();
        git::holds(&add).unwrap_or(false)
            && (!target.is_dir()
                || std::fs::create_dir_all(worktree.join("app"))
                    .and_then(|()| std::fs::rename(&target, worktree.join("app/target")))
                    .is_ok())
    }

    /// The worktree gone, its target kept; safe when there is none.
    pub(crate) fn worktree_close(&self) {
        let (worktree, target) = (self.state.join("worktree"), self.state.join("target"));
        let built = worktree.join("app/target");
        if built.is_dir() && !target.exists() {
            let _ = std::fs::rename(&built, &target);
        }
        let remove = [
            OsStr::new("worktree"),
            "remove".as_ref(),
            "--force".as_ref(),
        ];
        if worktree.exists()
            && !git::holds(&[&remove[..], &[worktree.as_os_str()]].concat()).unwrap_or(false)
        {
            let _ = std::fs::remove_dir_all(&worktree);
        }
        let _ = git::holds(&["worktree", "prune"]);
    }

    /// Cancels every unfinished run of the gate branch, and of its commit's publication, so no
    /// red tree's builds reach the cache.
    pub(crate) fn cancel(&self, runs: &Runs) {
        let Ok(repo) = self.repo() else { return };
        let (branch, of_commit) = match runs {
            Runs::OfBranch(branch) => (branch, String::new()),
            Runs::OfCommit(pushed, branch) => (
                branch,
                format!(
                    r#" or .head_sha == "{pushed}" or .display_title == "Publication cache {pushed}""#
                ),
            ),
        };
        let mut list = self.gh();
        list.args(["api", &format!("repos/{repo}/actions/runs?per_page=100"), "--jq"]).arg(format!(
            r#".workflow_runs[] | select(.status != "completed" and (.head_branch == "{branch}"{of_commit})) | .id"#
        ));
        let Ok(ids) = process::out(&mut list) else {
            eprintln!("train: could not list the runs of {branch} on {repo} to cancel them");
            return;
        };
        for id in ids.lines().filter_map(|id| id.parse::<u64>().ok()) {
            let mut cancel = self.gh();
            cancel.args([
                "api",
                &format!("repos/{repo}/actions/runs/{id}/cancel"),
                "-X",
                "POST",
            ]);
            if !process::succeeds(&mut cancel, std::process::Stdio::inherit()).unwrap_or(false) {
                eprintln!(
                    "train: could not cancel the run {id} on {repo}: cancel it by hand (gh run \
                     cancel {id} --repo {repo})"
                );
            }
        }
    }

    /// Each `gate/<commit>` branch on core, which only a train holding the lock pushes, so one is
    /// a stopped or killed train's: its runs cancelled and it deleted.
    pub(crate) fn sweep(&self) -> Result<(), Error> {
        let heads = git::out(&["ls-remote", "--heads", "core", "refs/heads/gate/*"])
            .map_err(|e| Error::Unrun(format!("could not reach core ({e})")))?;
        for (tip, name) in heads
            .lines()
            .filter_map(|line| line.split_once("\trefs/heads/"))
        {
            let commit = name.strip_prefix("gate/").map(Oid::parse);
            if let (Ok(tip), Some(Ok(_)), Ok(branch)) =
                (Oid::parse(tip), commit, GateBranch::parse(name))
            {
                eprintln!(
                    "train: core's {name} was left by a train stopped or killed: its runs are \
                     cancelled and it is deleted"
                );
                self.cancel(&Runs::OfCommit(tip, branch.clone()));
                delete(&branch);
            }
        }
        Ok(())
    }
}

/// Deletes the gate branch on core. One core does not hold (its push stopped before core took
/// it) is done: the delete is asked anyway, since a push killed after its pack was sent can land
/// just after.
fn delete(branch: &GateBranch) {
    let name = format!("refs/heads/{branch}");
    // Git's words untranslated, so "remote ref does not exist" reads in any locale.
    let mut push = git::cmd(&["push", "--quiet", "core", "--delete", &name]);
    push.env("LC_ALL", "C");
    let done = process::output(&mut push).is_ok_and(|out| {
        out.status.success()
            || String::from_utf8_lossy(&out.stderr).contains("remote ref does not exist")
    });
    if !done {
        eprintln!(
            "train: could not delete core's {branch}: delete it by hand (git push core --delete \
             {branch})"
        );
    }
}

fn slot(half: Half) -> usize {
    match half {
        Half::Tree => 0,
        Half::Live => 1,
    }
}

/// `owner/name` of a GitHub remote's address.
fn github_repo(url: &str) -> Option<String> {
    let path = url.split_once("github.com")?.1.strip_prefix([':', '/'])?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let plain = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
    };
    match path.split('/').collect::<Vec<_>>()[..] {
        [owner, name] if plain(owner) && plain(name) => Some(path.to_owned()),
        _ => None,
    }
}

/// A log line without the timestamp GitHub puts before it (`2026-10-20T10:00:00.1234567Z `).
fn untimed(line: &str) -> &str {
    let stamp = |s: &str| {
        s.len() > 11
            && s.bytes().enumerate().all(|(at, b)| match at {
                4 | 7 => b == b'-',
                10 => b == b'T',
                _ => b.is_ascii_digit() || (at > 10 && (b == b':' || b == b'.')),
            })
    };
    match line.split_once("Z ") {
        Some((head, rest)) if stamp(head) => rest,
        _ => line,
    }
}
