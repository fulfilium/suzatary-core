//! The verbs that run gate plans (`gate`, `bisect`, `nightly`, `check`) and the lock's
//! (`locked`), and what each reports.

use crate::bisect::{Search, Stuck};
use crate::data::Data;
use crate::gate_branch::GateBranch;
use crate::gate_run::{Depth, Growth, HOLDER, Kept, Logs, Ran, TRAIN_READS, Train, said};
use crate::gating::{OnRed, Plan, Push};
use crate::lock::Holder;
use crate::oid::Oid;
use crate::outcome::Check;
use crate::{Ended, Error, Half, LandOrder, Outcome, Reading, git, process};
use std::process::Command;

impl Train {
    /// A gate's start: inside `.#triage` with nothing beside it, the lock taken, origin synced,
    /// and the data its live rows are judged at read once.
    fn begin(&mut self, verb: &str, kept: Kept) -> Result<(), Error> {
        self.environment(verb)?;
        self.lock = Some(match self.holder(verb).take() {
            Err(Error::Busy(_)) if kept == Kept::Nightly => {
                eprintln!("train: a train is running here: the nightly gate waits for it");
                let state = self.state.clone();
                let wait = move || {
                    Holder {
                        state: &state,
                        verb: "nightly",
                    }
                    .wait()
                };
                self.sup
                    .until(wait)
                    .map_err(|signal| Error::Stopped(signal.as_raw()))??
            }
            held => held?,
        });
        if self.shell(&["sync"])? != 0 {
            return Err(said("origin could not be fetched (above)."));
        }
        let data = Data::of_checkout(&self.root)?;
        eprintln!(
            "train: the live half is judged at the stores' digest {}",
            data.short()
        );
        self.depth = Depth::Kept(kept, data);
        Ok(())
    }

    /// Refused outside `.#triage` or with an inherited variable a gate's steps read; the data
    /// tree's instance made absolute for the gates.
    fn environment(&mut self, verb: &str) -> Result<(), Error> {
        let var = |name| std::env::var(name).unwrap_or_default();
        if var("IN_NIX_SHELL").is_empty() || var("SUZATARY_SHELL") != "triage" {
            return Err(said(format!(
                "a gate runs inside the .#triage shell, whose tools are what it tests with: nix \
                 develop .#triage -c scripts/train.sh {verb}"
            )));
        }
        let sets = var("SUZATARY_SHELL_SETS");
        let shell = ["SUZATARY_SHELL", "SUZATARY_SHELL_SETS", "SUZATARY_INSTANCE"];
        let allowed: Vec<&str> = shell
            .into_iter()
            .chain(["SUZATARY_GATE_SERIAL"])
            .chain(TRAIN_READS)
            .chain(sets.split_whitespace())
            .collect();
        let reads = |name: &str| {
            ["SUZATARY_", "CARGO", "RUST", "PROPTEST_", "TYPST"]
                .iter()
                .any(|f| name.starts_with(f))
                || [
                    "NODE_OPTIONS",
                    "LD_PRELOAD",
                    "LD_LIBRARY_PATH",
                    "LIBRARY_PATH",
                ]
                .contains(&name)
        };
        let found: Vec<String> = std::env::vars_os()
            .filter_map(|(name, _)| name.into_string().ok())
            .filter(|name| reads(name) && !allowed.contains(&name.as_str()))
            .collect();
        if !found.is_empty() {
            let found = found.join(" ");
            return Err(said(format!(
                "the caller exported {found}, which the gate's steps read and the .#triage shell \
                 does not set, so the gate would test something beside the tree: unset {found} \
                 and gate again."
            )));
        }
        let link = self.root.join("instance.toml");
        if var("SUZATARY_INSTANCE").is_empty() && link.exists() {
            self.instance = link.canonicalize().ok();
        }
        Ok(())
    }

    /// `scripts/train.sh <verb>` under this train's lock: its status.
    fn shell(&mut self, verb: &[&str]) -> Result<u8, Error> {
        let mut cmd = Command::new(self.scripts.join("train.sh"));
        cmd.args(verb).env(HOLDER, std::process::id().to_string());
        self.attend(cmd)
    }

    fn train_commit() -> Result<Oid, Error> {
        git::commit_if("train")?
            .ok_or_else(|| said("there is no train; scripts/train.sh build <pr>... first."))
    }

    /// `gate [--full] [--remote github]`: the growth check, then both halves of the train, and the
    /// report.
    pub fn gate(&mut self, full: bool, remote: bool) -> Result<u8, Error> {
        self.begin("gate", if full { Kept::Full } else { Kept::Selective })?;
        let commit = Self::train_commit()?;
        match self.history(&commit) {
            Growth::Within => {}
            Growth::Refused(refused) => {
                for growth in &refused {
                    eprintln!("train: growth: {growth}");
                }
                return Err(said(
                    "the train's commits do not declare the growth above: build again with the \
                     pull request's description mended, or without it. Nothing was gated, and no \
                     row was written.",
                ));
            }
            Growth::Unrun(why) => {
                eprintln!("train: the growth check did not run ({why}); the halves go on");
            }
        }
        if remote {
            self.repo()?;
            self.sweep()?;
        }
        let plan = self.plan(&commit, &Half::BOTH, remote.then_some("gate/"), OnRed::Stop)?;
        self.run(plan)?;
        self.report(&commit)
    }

    /// Each half's reading and a red one's failing lines; the train's verdict.
    fn report(&self, commit: &Oid) -> Result<u8, Error> {
        let mut readings = [Reading::Unjudged; 2];
        for (half, reading) in Half::BOTH.into_iter().zip(&mut readings) {
            *reading = self.reading(half, commit)?;
            eprintln!("train: {} half: {reading}", half.name());
            let log = std::fs::read_to_string(self.log(half, commit)?).unwrap_or_default();
            if *reading == Reading::Agreed(Outcome::Red) && !log.is_empty() {
                eprint!("{}", failing_lines(&log));
            }
        }
        let logs = self.state.join("logs");
        let short = git::out(&["rev-parse", "--short", &commit.to_string()])?;
        let logs = logs.display();
        match LandOrder::of(LandOrder::verdict(readings)) {
            LandOrder::Green => {
                eprintln!("train: green: the train {short} may land (scripts/train.sh land)");
                Ok(0)
            }
            LandOrder::Finding => Err(said(format!(
                "red, where a half once passed this tree at the same stores, and no person set the \
                 red aside: the gate is not a function of the tree. {ASIDE} The logs are in \
                 {logs}/."
            ))),
            LandOrder::Unjudged => Err(said(format!(
                "a half has no verdict on {short}: gate again."
            ))),
            LandOrder::Red => Err(said(format!(
                "red: scripts/train.sh bisect finds the merge that broke it. {ASIDE} The logs \
                 are in {logs}/."
            ))),
        }
    }

    /// `bisect [--remote github]`: the longest prefix of a red train's merges its first red half
    /// passes, assuming red stays red.
    pub fn bisect(&mut self, remote: bool) -> Result<u8, Error> {
        self.begin("bisect", Kept::Selective)?;
        if remote {
            self.repo()?;
            self.sweep()?;
        }
        let train = Self::train_commit()?;
        let half = self.red_half(&train)?;
        if half == Half::Live {
            self.say_moved_stores(&train)?;
        }
        let base = git::out(&["merge-base", "origin/master", &train.to_string()])?;
        let range = format!("{base}..{train}");
        let mut merges = Vec::new();
        for commit in git::out(&["rev-list", "--first-parent", "--reverse", &range])?.lines() {
            let commit = Oid::parse(commit)?;
            if pr_of(&commit).is_some() {
                merges.push(commit);
            }
        }
        let n = merges.len();
        let Some(top) = merges.last().cloned() else {
            return Err(said(
                "the train merges no PR, so there is nothing to bisect.",
            ));
        };
        let name = half.name();
        let probe = |train: &mut Train, commit: &Oid| {
            let plan = train.plan(commit, &[half], remote.then_some("gate/"), OnRed::Stop)?;
            train.run(plan).map(|_| ())
        };
        let prs = |commits: &[Oid]| {
            commits
                .iter()
                .flat_map(prs_of)
                .collect::<Vec<_>>()
                .join(" ")
        };
        if top != train {
            if self.reading(half, &top)? == Reading::Unjudged {
                let n = pr_of(&top).unwrap_or_default();
                eprintln!("train: probe: every merge, through #{n}, without the release on top");
                probe(self, &top)?;
            }
            if self.reading(half, &top)? == Reading::Agreed(Outcome::Green) {
                println!("green in the {name} half: master and every merge of the train");
                println!("broke it: {}", subject(&train));
                let next = prs(&merges);
                println!(
                    "next: scripts/train.sh build {next}, then gate (the release is cut again)"
                );
                return Ok(0);
            }
        }
        eprintln!(
            "train: bisecting {n} merges over the {name} half: at most {} probes, each that half \
             of the merge gate (a prefix gated before is read from the table)",
            Search::most(n)
        );
        let found = Search { lo: 0, hi: n }.run(
            self,
            |train, mid| train.reading(half, &merges[mid - 1]),
            |train, mid| {
                let n = pr_of(&merges[mid - 1]).unwrap_or_default();
                eprintln!("train: probe: the first {mid} merges, through #{n}");
                probe(train, &merges[mid - 1])
            },
        )?;
        let green: Vec<String> = merges[..found.lo]
            .iter()
            .filter_map(pr_of)
            .map(|n| format!("#{n}"))
            .collect();
        let broke = &merges[found.hi - 1];
        println!(
            "green in the {name} half: master and the first {} merges {}",
            found.lo,
            green.join(" ")
        );
        println!("broke it: {}", subject(broke));
        // The broken merge's own PR is left out; the PRs it carried go on, to be judged alone.
        let next = merges.iter().flat_map(|commit| {
            prs_of(commit)
                .into_iter()
                .skip(usize::from(commit == broke))
        });
        println!(
            "next: scripts/train.sh build {}, then gate",
            next.collect::<Vec<_>>().join(" ")
        );
        Ok(0)
    }

    /// The train's first red half, or why there is nothing to bisect.
    fn red_half(&self, train: &Oid) -> Result<Half, Error> {
        let tree = self.reading(Half::Tree, train)?;
        let live = self.reading(Half::Live, train)?;
        match (LandOrder::of(LandOrder::verdict([tree, live])), tree) {
            (LandOrder::Red, Reading::Agreed(Outcome::Red)) => Ok(Half::Tree),
            (LandOrder::Red, _) => Ok(Half::Live),
            (LandOrder::Green, _) => Err(said("the train is green; there is nothing to bisect.")),
            (LandOrder::Unjudged, _) => Err(said(
                "the gate has not judged the train's HEAD in both halves: scripts/train.sh gate \
                 first.",
            )),
            (LandOrder::Finding, _) => Err(said(
                "the gate both passed and failed the train's tree: it is not a function of the \
                 tree, so no prefix's verdict can be trusted either.",
            )),
        }
    }

    /// `nightly`: origin/master's growth check, then both halves, full, neither stopping the other;
    /// a red half files a todo (`train.sh nightly-report`), and a refused growth fails it after the
    /// halves. It waits for a running train's lock.
    pub fn nightly(&mut self) -> Result<u8, Error> {
        self.begin("nightly", Kept::Nightly)?;
        let commit = git::commit("origin/master")?;
        let growth = self.history(&commit);
        let prefix = self.repo.is_some().then_some("gate/nightly-");
        let plan = self.plan(&commit, &Half::BOTH, prefix, OnRed::GoOn)?;
        self.run(plan)?;
        let commit_word = commit.to_string();
        let (mut report, mut silent) = (vec!["nightly-report", &commit_word], Vec::new());
        for half in Half::BOTH {
            match self.reading(half, &commit)? {
                Reading::Agreed(Outcome::Green) => {}
                Reading::Agreed(Outcome::Red) => report.push(half.name()),
                Reading::Unjudged | Reading::Finding => silent.push(half.name()),
            }
        }
        // A night with no red and a half that said nothing did not pass: the report would say so.
        let status = if report.len() > 2 || silent.is_empty() {
            self.shell(&report)?
        } else {
            0
        };
        if !silent.is_empty() {
            return Err(said(format!(
                "the {} half gave no verdict on origin/master {}; the logs are in {}/.",
                silent.join(" "),
                commit.short(),
                self.state.join("logs").display()
            )));
        }
        match growth {
            Growth::Within => Ok(status),
            Growth::Refused(refused) => Err(said(format!(
                "origin/master's commits do not declare their growth: {}",
                refused.join("; ")
            ))),
            Growth::Unrun(why) => Err(said(format!(
                "the growth check did not run on origin/master ({why}), so no train's growth is \
                 checked until it does"
            ))),
        }
    }

    /// `check <pr>`: the runners' run of the pull request's head merged on origin/master, posted on
    /// the pull request; no lock, no row.
    pub fn check(&mut self, pr: u32) -> Result<u8, Error> {
        self.depth = Depth::Unkept;
        self.repo()?;
        let pull = format!("+refs/pull/{pr}/head:refs/check/pr{pr}");
        if !git::act(&["fetch", "--quiet", "origin", "master"])? {
            return Err(said("origin could not be fetched."));
        }
        if !git::act(&["fetch", "--quiet", "origin", &pull])? {
            return Err(said(format!("origin has no refs/pull/{pr}/head.")));
        }
        let head = git::commit(&format!("refs/check/pr{pr}"))?;
        let master = git::commit("origin/master")?;
        let mut view = Command::new(&self.seams.gh);
        view.args([
            "pr",
            "view",
            &pr.to_string(),
            "--json",
            "title,url,headRefOid",
        ])
        .args(["--jq", "[.title, .url, .headRefOid] | @tsv"]);
        let meta =
            process::out(&mut view).map_err(|_| said(format!("gh could not read #{pr}.")))?;
        let current = meta.split('\t').nth(2).unwrap_or_default();
        if current != head.to_string() {
            return Err(said(format!(
                "#{pr}: refs/pull/{pr}/head is not GitHub's head {current} yet."
            )));
        }
        let what = format!(
            "The tree half of {} on master {}",
            head.short(),
            master.short()
        );
        let (master, head_word) = (master.to_string(), head.to_string());
        let merge = [
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            &master,
            &head_word,
        ];
        let merged = process::output(&mut git::cmd(&merge))?;
        let said_tree = String::from_utf8_lossy(&merged.stdout).into_owned();
        let mut lines = said_tree.lines();
        let tree = lines.next().unwrap_or_default();
        if !merged.status.success() {
            let files: Vec<&str> = lines.filter(|l| !l.is_empty()).collect();
            self.post(
                pr,
                &format!("{what} conflicts with master in: {}.", files.join(" ")),
            )?;
            return Ok(1);
        }
        let message = format!("Check #{pr}");
        let mut commit = git::cmd(&[
            "commit-tree",
            tree,
            "-p",
            &master,
            "-p",
            &head_word,
            "-m",
            &message,
        ]);
        let merge = Oid::parse(&process::out(commit.envs(crate::publication::AUTHOR))?)?;
        let log = self
            .state
            .join("logs")
            .join(format!("check-pr{pr}-{head}.log"));
        let _ = std::fs::create_dir_all(self.state.join("logs"));
        let _ = std::fs::write(&log, "");
        self.logs = Logs::Check(log.clone());
        let short = head.short();
        let push = Push {
            branch: GateBranch::parse(&format!("gate/pr{pr}-{short}"))?,
            subject: format!("Check #{pr} {short}"),
            trailer: format!("Check: {head}"),
        };
        let polls = self.polls();
        let Ran {
            ended: [tree, _],
            run,
        } = self.run(Plan::check(merge, push, polls))?;
        let url = run.map_or_else(|| "no run".to_owned(), |run| run.url);
        let log = std::fs::read_to_string(&log).unwrap_or_default();
        let verdict = verdict(&what, tree, &url, &log);
        self.post(pr, &verdict)?;
        // As the shell's `check`: 0 green, 1 red or kept on the box, 2 no verdict.
        Ok(match tree {
            Ended::Judged(Outcome::Green) => 0,
            Ended::Judged(Outcome::Red) | Ended::Refused(_) => 1,
            Ended::Unjudged => 2,
        })
    }

    /// A comment on the pull request, and its first line here.
    fn post(&self, pr: u32, text: &str) -> Result<(), Error> {
        eprintln!("train: #{pr}: {}", text.lines().next().unwrap_or_default());
        let mut comment = Command::new(&self.seams.gh);
        comment.args(["pr", "comment", &pr.to_string(), "--body", text]);
        process::out(&mut comment)
            .map(|_| ())
            .map_err(|_| said(format!("could not comment on #{pr} (above).")))
    }

    /// `locked <verb> -- <command>...`: the command run under the train's lock, a signal passed on
    /// to it; its status.
    pub fn locked(&mut self, verb: &str, command: &[String]) -> Result<u8, Error> {
        self.lock = Some(self.holder(verb).take()?);
        let mut cmd = Command::new(&command[0]);
        cmd.args(&command[1..])
            .env(HOLDER, std::process::id().to_string());
        self.attend(cmd)
    }
}

impl From<Stuck> for Error {
    fn from(stuck: Stuck) -> Error {
        said(match stuck {
            Stuck::Unjudged(n) => {
                format!(
                    "the half gave no verdict on the first {n} merges, so the search stops here."
                )
            }
            Stuck::Finding(n) => format!(
                "the gate both passed and failed the first {n} merges' tree: it is not a function \
                 of the tree, so the search stops here."
            ),
        })
    }
}

/// How a red traced to a cause outside the tree stops stopping a land.
const ASIDE: &str = "A red whose cause is outside the tree is set aside at a terminal: \
                     scripts/train.sh set-aside <half> --because <task> \"<words>\".";

/// What `check` posts of the runners' end of the tree half.
fn verdict(what: &str, tree: Ended, url: &str, log: &str) -> String {
    let nothing = "nothing went to GitHub.";
    match tree {
        Ended::Judged(Outcome::Green) => format!("{what} is green on GitHub's runners: {url}"),
        Ended::Judged(Outcome::Red) => {
            let lines = failing_lines(log);
            format!("{what} is red on GitHub's runners: {url}\n\n```\n{lines}```")
        }
        Ended::Refused(Check::Leak) => {
            format!("{what} did not pass the leak guard on the box: {nothing}")
        }
        Ended::Refused(Check::Held) => {
            format!("{what} holds an admission no publication of master holds yet: {nothing}")
        }
        Ended::Refused(Check::Unrun) => format!(
            "{what} could not be checked or sent from the box (its words are on the box): \
             {nothing} Check again."
        ),
        Ended::Unjudged => {
            format!("{what} gave no verdict on GitHub's runners: {url}. Check again.")
        }
    }
}

fn subject(commit: &Oid) -> String {
    git::out(&["log", "-1", "--format=%s", &commit.to_string()]).unwrap_or_default()
}

/// The pull request a train's merge names in its subject, `Merge #<n>: ...`.
fn pr_of(commit: &Oid) -> Option<String> {
    let subject = subject(commit);
    let n = subject.strip_prefix("Merge #")?.split_once(':')?.0;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| n.to_owned())
}

/// The merge's pull request, then those its `Carries:` trailer names.
fn prs_of(commit: &Oid) -> Vec<String> {
    let format = "--format=%(trailers:key=Carries,valueonly,separator= )";
    let carried = git::out(&["log", "-1", format, &commit.to_string()]).unwrap_or_default();
    pr_of(commit)
        .into_iter()
        .chain(carried.split_whitespace().map(str::to_owned))
        .collect()
}

/// A line without its colours (`ESC [ <digits and ;> m`).
fn uncoloured(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find("\x1b[") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 2..];
        let tail = tail.trim_start_matches(|c: char| c.is_ascii_digit() || c == ';');
        rest = tail.strip_prefix('m').unwrap_or(tail);
    }
    out + rest
}

/// Per `--- job` section of a log: the gate's failing-steps block, else its last twenty error
/// lines, each once, uncoloured.
#[must_use]
pub(crate) fn failing_lines(log: &str) -> String {
    let mut out = String::new();
    let (mut block, mut inside, mut under) = (false, false, 0);
    let mut errors: Vec<String> = Vec::new();
    let flush = |block: bool, errors: &mut Vec<String>, out: &mut String| {
        if !block {
            for line in &errors[errors.len().saturating_sub(20)..] {
                out.push_str(line);
                out.push('\n');
            }
        }
        errors.clear();
    };
    let keep = |line: String, errors: &mut Vec<String>| {
        if !errors.contains(&line) {
            errors.push(line);
        }
    };
    for line in log.lines().map(uncoloured) {
        let header = line.starts_with("--- job ");
        if header || line == "==> the failing steps' last lines" {
            if header {
                flush(block, &mut errors, &mut out);
            }
            (block, inside, under) = (!header, !header, 0);
        } else if inside
            && ["size: ", "free after the gate: ", "##["]
                .iter()
                .any(|p| line.starts_with(p))
        {
            inside = false;
        }
        if header || inside {
            out.push_str(&line);
            out.push('\n');
        } else if line.contains('✖') || line.contains("not ok") {
            keep(line, &mut errors);
            under = 6;
        } else if under > 0
            && ["Error", "waited", "last:"]
                .iter()
                .any(|w| line.contains(w))
        {
            keep(line, &mut errors);
            under -= 1;
        } else {
            if under > 0 && !line.starts_with(char::is_whitespace) {
                under = 0;
            }
            if ["FAILED", "panicked"].iter().any(|w| line.contains(w))
                || line.starts_with("error")
                || line.starts_with("##[error]")
            {
                keep(line, &mut errors);
            }
        }
    }
    flush(block, &mut errors, &mut out);
    out
}
