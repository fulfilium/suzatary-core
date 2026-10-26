//! The merge queue (`scripts/train.sh`), run as the real script against a throwaway
//! origin: a bare repository whose `refs/pull/<n>/head` are the pull requests, a stub
//! `gh` that answers their titles and plays a Codespace (a clone of origin its `codespace
//! ssh` runs commands in), and a stub gate whose halves' verdicts are functions of the tree.

use super::{Scratch, repo};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// An origin, an author's clone that writes the pull requests and moves master, and the
/// operator's clone the train is built in.
struct Train {
    scratch: Scratch,
}

/// The stub `gh`. `pr view <n> --json title,url,headRefOid --jq …` answers the title, URL and
/// head commit in `prs/<n>`, tab-separated, and `pr view <n> --json body …` the description in
/// `prs/<n>.body`, empty when there is none; `codespace ssh -c <name> -- <command>` runs the
/// command in the `codespace` clone, or fails as a dropped connection when `GH_SSH_DOWN` is set
/// or when it is the `GH_SSH_DROP_AT`th such call; `codespace ssh -c <name> -- -t …`,
/// the keep-alive, is logged as `keep`, writes its pid to `keepers` and holds on
/// until it is stopped, unless the connection is down; `codespace stop -c <name>`
/// is logged. The first command of a run waits on the `awake` fifo for the first
/// keep-alive (ten seconds at most, so a train that starts none fails rather than
/// hangs), so a gate's keep-alive is always the first call it logs. The stub holds
/// none of the train's lock: what it starts in the Codespace is not the train.
///
/// It also plays GitHub's API over the gate branches pushed to `core.git`, logging each call to
/// `github.log`: `api …/actions/runs?head_sha=<sha>…` answers nothing until core holds `<sha>`,
/// then runs the stub gate's tree half once in `runner-<sha>`, a checkout of that commit alone, as
/// gate.yml's jobs do, and answers the run's id (the sha); `api …/actions/runs/<id>` answers
/// `completed <conclusion>`, success where the gate passed and failure where it failed;
/// `…/runs/<id>/jobs…` answers `7 gate (tests)` for a failed gate, whose `red` step failed; and
/// `…/jobs/7/logs` the gate's output, each line stamped with a time as GitHub's are. With
/// `GH_RUN=cancelled` the run is cancelled, and with `GH_RUN=lost` it fails with no red step, as
/// a lost runner's does, and with `GH_RUN=running` it never ends; none of them runs the gate.
const GH: &str = r#"#!/bin/sh
exec 9>&-
here="$(cd "$(dirname "$0")/.." && pwd)"
if [ "$1" = api ]; then
  echo "api $2" >>"$here/github.log"
  case "$2" in
    repos/*/actions/runs\?head_sha=*)
      sha="${2#*head_sha=}" && sha="${sha%%&*}"
      git --git-dir="$here/core.git" cat-file -e "$sha^{commit}" 2>/dev/null || exit 0
      if [ ! -e "$here/runs/$sha" ]; then
        mkdir -p "$here/runs"
        case "$GH_RUN" in
          cancelled) echo "completed cancelled" >"$here/runs/$sha" ;;
          lost) echo "completed failure" >"$here/runs/$sha" ;;
          running) echo "in_progress " >"$here/runs/$sha" ;;
          *)
            git clone -q "$here/core.git" "$here/runner-$sha" && git -C "$here/runner-$sha" checkout -q --detach "$sha"
            if (cd "$here/runner-$sha" && "$here/bin/gate" --no-live --steps cloud) >"$here/runs/$sha.log" 2>&1; then
              echo "completed success" >"$here/runs/$sha"
            else
              echo "completed failure" >"$here/runs/$sha" && : >"$here/runs/$sha.red"
            fi ;;
        esac
      fi
      echo "$sha" ;;
    repos/*/actions/runs/*/jobs*)
      run="${2#*/runs/}" && run="${run%%/*}"
      [ ! -e "$here/runs/$run.red" ] || echo "7 gate (tests)" ;;
    repos/*/actions/runs/*) cat "$here/runs/${2##*/}" ;;
    repos/*/actions/jobs/7/logs) sed 's/^/2026-10-20T10:00:00.1234567Z /' "$(ls -t "$here"/runs/*.log | head -n 1)" ;;
    *) exit 64 ;;
  esac
  exit 0
fi
case "$1 $2" in
  "pr view")
    if [ "$5" = body ]; then cat "$here/prs/$3.body" 2>/dev/null; exit 0; fi
    exec cat "$here/prs/$3" ;;
  "codespace ssh")
    if [ "$6" = -t ]; then
      echo "keep $4" >>"$here/codespace.log"
      [ ! -p "$here/awake" ] || : >"$here/awake"
      [ -z "$GH_SSH_DOWN" ] || exit 255
      echo $$ >>"$here/keepers"
      exec sleep 600
    fi
    [ ! -p "$here/awake" ] || { timeout 10 cat "$here/awake" >/dev/null; rm "$here/awake"; }
    echo "ssh $4" >>"$here/codespace.log"
    [ -z "$GH_SSH_DOWN" ] || { echo "connection reset" >&2; exit 255; }
    [ "$(grep -c '^ssh ' "$here/codespace.log")" != "${GH_SSH_DROP_AT:-0}" ] || { echo "connection reset" >&2; exit 255; }
    shift 5
    CODESPACE_VSCODE_FOLDER="$here/codespace" exec bash -c "$*" ;;
  "codespace stop") echo "stop $4" >>"$here/codespace.log" ;;
  *) exit 64 ;;
esac
"#;

/// The stub merge gate, `[--full] --no-live` or `[--full] --only-live`, a full run noted in
/// `<TRAIN_PROBES>.full`. With `GATE_HOLD` set, the tree half says so on the `<GATE_HOLD>.up`
/// fifo and waits on `<GATE_HOLD>` to go on, and with `GATE_HOLD_LIVE` the live half does the
/// same. A full run of a tree holding `missed` fails, as a selection that missed a step would.
/// With `GATE_RED_TEXT` set, the tree half is red on a tree whose CHANGELOG.md holds that text.
/// Each run writes the environment it was handed to `<TRAIN_PROBES>.env`, and its arguments, a
/// line, to `<TRAIN_PROBES>.args`. Run with `--steps` naming `changelog`, the box's part of a
/// tree half on GitHub's runners, it is red on a tree holding `broken-box` and on no other.
const GATE: &str = r#"#!/bin/sh
env >"$TRAIN_PROBES.env"
echo "$*" >>"$TRAIN_PROBES.args"
full=""
if [ "$1" = --full ]; then full=1; shift; echo "$1 $(git rev-parse HEAD)" >>"$TRAIN_PROBES.full"; fi
echo "$1 $(git rev-parse HEAD) $(pwd)" >>"$TRAIN_PROBES"
[ -z "$full" ] || [ ! -e missed ] || { echo "missed is in the tree" >&2; exit 1; }
[ -z "$GATE_HOLD" ] || [ "$1" != --no-live ] || { echo up >"$GATE_HOLD.up"; cat "$GATE_HOLD" >/dev/null; }
[ -z "$GATE_HOLD_LIVE" ] || [ "$1" != --only-live ] || { echo up >"$GATE_HOLD_LIVE.up"; cat "$GATE_HOLD_LIVE" >/dev/null; }
case "$1" in
  --no-live)
    bad=broken
    case "$2 $3" in "--steps "*changelog*) bad=broken-box ;; esac ;;
  --only-live) bad=broken-live ;;
  *) exit 64 ;;
esac
[ -z "$GATE_RED_TEXT" ] || [ "$1" != --no-live ] || ! grep -qs "$GATE_RED_TEXT" CHANGELOG.md || bad=CHANGELOG.md
test -e "$bad" || exit 0
printf '==> the failing steps'"'"' last lines\n--- stub\n%s is in the tree\n' "$bad" >&2
exit 1
"#;

/// The stub public-tree script: `<out> <rev>` writes `<rev>`'s tree without its `private/`,
/// and `--check --strict <dir>` exits 1, naming the file, where a file of it says `leak`. With
/// `PUBLIC_UNWRITTEN` set no tree is written, and with `PUBLIC_UNCHECKED` the check exits 2,
/// the real one's "could not run whole".
const PUBLIC: &str = r#"#!/bin/sh
if [ "$1" = --check ]; then
  [ "$2" = --strict ] || exit 64
  [ -z "$PUBLIC_UNCHECKED" ] || { echo "public-tree: a list is missing" >&2; exit 2; }
  hits="$(grep -rl leak "$3")" || exit 0
  echo "$hits"
  exit 1
fi
[ -z "$PUBLIC_UNWRITTEN" ] || { echo "public-tree: no space left" >&2; exit 1; }
mkdir -p "$1" && git archive "$2" | tar -x -C "$1" && rm -rf "$1/private"
"#;

const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0]\n\n- the first release\n";

impl Train {
    fn new(name: &str) -> Self {
        let train = Train {
            scratch: Scratch::new(&format!("train-{name}")),
        };
        let root = &*train.scratch;
        run_git(
            root,
            &["init", "-q", "--bare", "-b", "master", "origin.git"],
        );
        run_git(root, &["init", "-q", "-b", "master", "author"]);
        let author = train.author();
        run_git(&author, &["remote", "add", "origin", "../origin.git"]);
        write(&author, "CHANGELOG.md", CHANGELOG);
        write(&author, "README.md", "a line\n");
        run_git(&author, &["add", "-A"]);
        run_git(&author, &["commit", "-q", "-m", "seed"]);
        run_git(&author, &["push", "-q", "origin", "master"]);
        run_git(root, &["clone", "-q", "origin.git", "work"]);
        run_git(root, &["clone", "-q", "origin.git", "codespace"]);
        // The repository deployments run, populated once before the train published anything,
        // holding every path these tests land (empty), so a publication holds no new path
        // unless a test means it to.
        run_git(root, &["init", "-q", "--bare", "-b", "master", "core.git"]);
        let blob = run_git(&author, &["hash-object", "-w", "/dev/null"]);
        let tree = |entries: &str| {
            let mut cmd = isolated(Command::new("git"));
            cmd.args(["mktree"])
                .current_dir(&author)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped());
            let mut child = cmd.spawn().expect("git mktree runs");
            std::io::Write::write_all(
                &mut child.stdin.take().expect("its stdin"),
                entries.as_bytes(),
            )
            .expect("the entries write");
            let out = child.wait_with_output().expect("git mktree ends");
            assert!(out.status.success(), "git mktree");
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        };
        let docs = tree(&format!("100644 blob {blob}\tnotes.md\n"));
        let known = [
            "CHANGELOG.md",
            "README.md",
            "a.txt",
            "b.txt",
            "broken",
            "broken-live",
            "c.txt",
            "start.txt",
        ]
        .iter()
        .map(|path| format!("100644 blob {blob}\t{path}\n"))
        .chain([format!("040000 tree {docs}\tdocs\n")])
        .collect::<String>();
        let known = tree(&known);
        let populated = run_git(&author, &["commit-tree", &known, "-m", "populated once"]);
        run_git(
            &author,
            &[
                "push",
                "-q",
                "../core.git",
                &format!("{populated}:refs/heads/master"),
            ],
        );
        run_git(&train.work(), &["remote", "add", "core", "../core.git"]);

        let bin = root.join("bin");
        std::fs::create_dir_all(root.join("prs")).expect("prs dir");
        // The tree half is green unless the tree holds `broken`, the live half unless it
        // holds `broken-live`; every run is counted by its flag. A red half ends with the
        // real gate's last section.
        for (stub, body) in [("gh", GH), ("gate", GATE), ("public", PUBLIC)] {
            let _ = suzatary_testkit::fake_program(&bin, stub, body);
        }
        run(Command::new("mkfifo").arg(root.join("awake")), "mkfifo");
        train
    }

    fn author(&self) -> PathBuf {
        self.scratch.join("author")
    }

    fn work(&self) -> PathBuf {
        self.scratch.join("work")
    }

    /// Pull request `n`: `files` written on a branch from origin's master, pushed as
    /// `refs/pull/<n>/head`, and its title and head given to the stub `gh`.
    fn pr(&self, n: u32, title: &str, files: &[(&str, &str)]) {
        run_git(&self.author(), &["fetch", "-q", "origin"]);
        self.pr_from(n, "origin/master", title, files);
    }

    /// Pull request `n`, stacked: its branch starts from pull request `on`'s.
    fn stacked(&self, n: u32, on: u32, title: &str, files: &[(&str, &str)]) {
        self.pr_from(n, &format!("pr{on}"), title, files);
    }

    /// Pull request `n`: `files` written on a branch from `from`, pushed and named.
    fn pr_from(&self, n: u32, from: &str, title: &str, files: &[(&str, &str)]) {
        let author = self.author();
        run_git(&author, &["checkout", "-q", "-B", &format!("pr{n}"), from]);
        for (path, content) in files {
            write(&author, path, content);
        }
        run_git(&author, &["add", "-A"]);
        run_git(&author, &["commit", "-q", "-m", title]);
        run_git(
            &author,
            &[
                "push",
                "-q",
                "-f",
                "origin",
                &format!("HEAD:refs/pull/{n}/head"),
            ],
        );
        let head = run_git(&author, &["rev-parse", "HEAD"]);
        self.names(n, title, &head);
    }

    /// What the stub `gh` says of pull request `n`: its title, and `head` as its head commit.
    fn names(&self, n: u32, title: &str, head: &str) {
        std::fs::write(
            self.scratch.join("prs").join(n.to_string()),
            format!("{title}\thttps://github.invalid/pull/{n}\t{head}\n"),
        )
        .expect("the stub's answer writes");
    }

    /// Pull request `n`'s head on origin.
    fn head(&self, n: u32) -> String {
        run_git(
            &self.scratch.join("origin.git"),
            &["rev-parse", &format!("refs/pull/{n}/head")],
        )
    }

    /// Master moves on origin by one commit writing `files`.
    fn master_moves(&self, files: &[(&str, &str)]) {
        let author = self.author();
        run_git(&author, &["fetch", "-q", "origin"]);
        run_git(
            &author,
            &["checkout", "-q", "-B", "master", "origin/master"],
        );
        for (path, content) in files {
            write(&author, path, content);
        }
        run_git(&author, &["add", "-A"]);
        run_git(&author, &["commit", "-q", "-m", "master moves"]);
        run_git(&author, &["push", "-q", "origin", "master"]);
    }

    /// Master moves to `files` as the release `version`: its commit carries the `Release:`
    /// trailer the train reads the last release from.
    fn master_releases(&self, files: &[(&str, &str)], version: &str) {
        let author = self.author();
        run_git(&author, &["fetch", "-q", "origin"]);
        run_git(
            &author,
            &["checkout", "-q", "-B", "master", "origin/master"],
        );
        for (path, content) in files {
            write(&author, path, content);
            if Path::new(path).extension().is_some_and(|e| e == "sh") {
                run(
                    Command::new("chmod").args(["+x"]).arg(author.join(path)),
                    "chmod",
                );
            }
        }
        run_git(&author, &["add", "-A"]);
        run_git(
            &author,
            &[
                "commit",
                "-q",
                "-m",
                &format!("release: {version}"),
                "-m",
                &format!("Release: {version}"),
            ],
        );
        run_git(&author, &["push", "-q", "origin", "master"]);
    }

    /// Origin's master.
    fn origin_master(&self) -> String {
        run_git(&self.scratch.join("origin.git"), &["rev-parse", "master"])
    }

    /// `scripts/train.sh <args>` in the operator's clone, with the stub gate and `gh`.
    fn train(&self, args: &[&str]) -> Output {
        self.train_with(args, &[])
    }

    /// `train` with `env` set too.
    fn train_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        self.command(args, env).output().expect("train.sh runs")
    }

    /// The `scripts/train.sh <args>` that `train_with` runs: in the `.#triage` shell as the
    /// train sees it, with none of the variables a gate's steps read inherited from this test's
    /// own environment (cargo's, or a gate's that runs these tests).
    fn command(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let bin = self.scratch.join("bin");
        let mut cmd = isolated(Command::new(repo().join("scripts").join("train.sh")));
        for (name, _) in std::env::vars_os() {
            let name = name.to_string_lossy();
            if ["SUZATARY_", "CARGO", "RUST", "PROPTEST_", "TYPST"]
                .iter()
                .any(|family| name.starts_with(family))
                || [
                    "NODE_OPTIONS",
                    "LD_PRELOAD",
                    "LD_LIBRARY_PATH",
                    "LIBRARY_PATH",
                ]
                .contains(&&*name)
            {
                cmd.env_remove(&*name);
            }
        }
        cmd.args(args)
            .current_dir(self.work())
            .env("IN_NIX_SHELL", "impure")
            .env("SUZATARY_SHELL", "triage")
            .env("SUZATARY_SHELL_SETS", "")
            .env("SUZATARY_TRAIN_GATE", bin.join("gate"))
            .env("SUZATARY_TRAIN_GH", bin.join("gh"))
            .env("SUZATARY_TRAIN_PUBLIC", bin.join("public"))
            .env("SUZATARY_TRAIN_BIN", super::binary::path())
            .env("TRAIN_PROBES", self.probes_file())
            .env("SUZATARY_TRAIN_POLL", "0")
            .envs(env.iter().copied());
        cmd
    }

    /// `train` through a pseudo-terminal (`script`), as the operator runs it at one.
    fn at_a_terminal(&self, args: &[&str]) -> Output {
        let train = self.command(args, &[]);
        let line = std::iter::once(train.get_program())
            .chain(train.get_args())
            .map(|word| format!("'{}'", word.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");
        let mut cmd = isolated(Command::new("script"));
        cmd.args(["-qec", &line, "/dev/null"])
            .current_dir(self.work())
            .envs(
                train
                    .get_envs()
                    .filter_map(|(key, value)| Some((key.to_owned(), value?.to_owned()))),
            );
        cmd.output().expect("script runs")
    }

    /// Where each run of `flag` ran: the directories, in order.
    fn ran_in(&self, flag: &str) -> Vec<PathBuf> {
        std::fs::read_to_string(self.probes_file())
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let mut words = line.splitn(3, ' ');
                let dir = PathBuf::from(words.nth(2)?);
                line.starts_with(&format!("{flag} "))
                    .then(|| dir.canonicalize().unwrap_or(dir))
            })
            .collect()
    }

    /// What the stub Codespace was asked, one line per call.
    fn codespace_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.scratch.join("codespace.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The stub keep-alives still running: their pids, a zombie counted as gone.
    fn keepers_running(&self) -> Vec<String> {
        std::fs::read_to_string(self.scratch.join("keepers"))
            .unwrap_or_default()
            .lines()
            .filter(|pid| {
                std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
                    stat.rsplit_once(") ")
                        .is_some_and(|(_, rest)| !rest.starts_with('Z'))
                })
            })
            .map(str::to_string)
            .collect()
    }

    /// Asserts the gate held the Codespace awake and left no keep-alive behind.
    fn kept_awake_and_let_go(&self) {
        let calls = self.codespace_calls();
        assert_eq!(
            calls.first().map(String::as_str),
            Some("keep cs"),
            "a keep-alive was started before anything ran: {calls:?}"
        );
        assert_eq!(
            self.keepers_running(),
            Vec::<String>::new(),
            "no keep-alive outlives the gate"
        );
    }

    /// The refs origin holds under `refs/heads/train/`.
    fn train_refs_on_origin(&self) -> String {
        run_git(
            &self.scratch.join("origin.git"),
            &["for-each-ref", "refs/heads/train/"],
        )
    }

    fn probes_file(&self) -> PathBuf {
        self.scratch.join("probes")
    }

    /// How many times the gate has run with `flag`.
    fn probes(&self, flag: &str) -> usize {
        std::fs::read_to_string(self.probes_file())
            .unwrap_or_default()
            .lines()
            .filter(|line| line.split(' ').next() == Some(flag))
            .count()
    }

    /// `git <args>` in the repository deployments run, its stdout trimmed.
    fn core(&self, args: &[&str]) -> String {
        run_git(&self.scratch.join("core.git"), args)
    }

    /// `git <args>` in the operator's clone, its stdout trimmed.
    fn git(&self, args: &[&str]) -> String {
        run_git(&self.work(), args)
    }

    /// The train's merges, oldest first, by subject.
    fn merges(&self) -> Vec<String> {
        self.git(&[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%s",
            "origin/master..train",
        ])
        .lines()
        .map(str::to_string)
        .collect()
    }
}

/// A command that reads no global or system git configuration and commits as one
/// fixed identity, so the operator's own hooks and settings cannot reach the fixture.
fn isolated(mut cmd: Command) -> Command {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t");
    cmd
}

fn run(cmd: &mut Command, what: &str) -> String {
    let out = cmd.output().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(
        out.status.success(),
        "{what}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_git(cwd: &Path, args: &[&str]) -> String {
    let mut cmd = isolated(Command::new("git"));
    run(cmd.args(args).current_dir(cwd), &format!("git {args:?}"))
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("parent dirs");
    std::fs::write(&path, content).unwrap_or_else(|e| panic!("{rel}: {e}"));
}

fn changelog_with(entry: &str) -> String {
    CHANGELOG.replace(
        "## [Unreleased]\n\n",
        &format!("## [Unreleased]\n\n- {entry}\n\n"),
    )
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

#[test]
fn train_build_merges_by_head_joins_the_changelog_and_skips_a_conflict() {
    let t = Train::new("build");
    // Each conflicts with master, so it is skipped whatever order the PRs merge in.
    t.pr(3, "the third", &[("README.md", "three\n")]);
    // An entry added and a released one rewritten: not a join of additions.
    let rewritten = changelog_with("four").replace("the first release", "a rewrite");
    t.pr(4, "the fourth", &[("CHANGELOG.md", &rewritten)]);
    let moved = CHANGELOG.replace("the first release", "master's rewrite");
    t.master_moves(&[("README.md", "master's\n"), ("CHANGELOG.md", &moved)]);
    let entry = |line: &str| changelog_with(line).replace("the first release", "master's rewrite");
    let long = "a title long enough that the merge subject must be cut to fit seventy-two";
    t.pr(
        1,
        "the first",
        &[("CHANGELOG.md", &entry("one")), ("a.txt", "a\n")],
    );
    t.pr(
        2,
        long,
        &[("CHANGELOG.md", &entry("two")), ("b.txt", "b\n")],
    );

    let out = t.train(&["build", "3", "4", "2", "1"]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "build: {}{stdout}", text(&out.stderr));

    let merges = t.merges();
    assert_eq!(
        merges.len(),
        2,
        "two merges, the conflicts skipped: {merges:?}"
    );
    let (first, second) = if t.head(1) < t.head(2) {
        (1, 2)
    } else {
        (2, 1)
    };
    assert!(
        merges[0].starts_with(&format!("Merge #{first}:"))
            && merges[1].starts_with(&format!("Merge #{second}:")),
        "PRs merge in the order of their heads' names: {merges:?}"
    );
    let long_merge = merges
        .iter()
        .find(|m| m.starts_with("Merge #2: a title long"))
        .expect("#2's merge");
    assert!(long_merge.chars().count() <= 72, "cut to 72: {long_merge}");
    let body = t.git(&["log", "-1", "--format=%b", "train^1"]);
    assert!(
        body.contains(&format!("https://github.invalid/pull/{first}")),
        "the URL: {body}"
    );
    let joined = stdout
        .lines()
        .find(|l| l.starts_with(&format!("#{second}")))
        .unwrap_or_default();
    assert!(
        joined.contains("joined CHANGELOG.md"),
        "the second merge joins: {stdout}"
    );

    let changelog = t.git(&["show", "train:CHANGELOG.md"]);
    let unreleased = &changelog[..changelog.find("## [0.1.0]").expect("the release")];
    assert!(
        unreleased.contains("- one") && unreleased.contains("- two"),
        "both entries under Unreleased:\n{changelog}"
    );
    assert!(!changelog.contains("<<<<<<<"), "no markers:\n{changelog}");

    for (n, file) in [("#3", "README.md"), ("#4", "CHANGELOG.md")] {
        let row = stdout
            .lines()
            .find(|l| l.starts_with(n))
            .unwrap_or_else(|| panic!("a row for {n}:\n{stdout}"));
        assert!(row.contains("skipped") && row.contains(file), "{row}");
    }
    let skipped = std::fs::read_to_string(t.work().join(".git/train/skipped")).expect("recorded");
    assert!(
        skipped.contains("#3") && skipped.contains("#4"),
        "{skipped}"
    );
    assert_eq!(
        t.git(&["status", "--porcelain"]),
        "",
        "the checkout is left clean"
    );
    assert_eq!(t.git(&["branch", "--show-current"]), "train");
}

#[test]
fn train_build_that_merges_none_of_its_prs_fails_naming_them() {
    let t = Train::new("build-none");
    t.pr(1, "the first", &[("README.md", "one\n")]);
    t.master_moves(&[("README.md", "master's\n")]);
    let out = t.train(&["build", "1"]);
    let said = text(&out.stderr);
    assert!(
        !out.status.success(),
        "a train of nothing must not read as built"
    );
    assert!(
        said.contains("no PR merged") && said.contains("#1"),
        "{said}"
    );
    assert_eq!(t.git(&["rev-parse", "train"]), t.origin_master());

    // A PR already on master merges nothing either.
    t.pr(2, "the second", &[("b.txt", "b\n")]);
    assert!(t.train(&["build", "2"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());
    let again = t.train(&["build", "2"]);
    assert!(
        !again.status.success() && text(&again.stderr).contains("#2"),
        "a train of a landed PR is master: {}",
        text(&again.stderr)
    );
}

/// The `<key>` trailer values of the train's merge of PR `n`, space-separated.
fn merge_trailer(t: &Train, n: u32, key: &str) -> String {
    let merge = t
        .git(&[
            "log",
            "--first-parent",
            "--format=%H %s",
            "origin/master..train",
        ])
        .lines()
        .find(|line| line.contains(&format!(" Merge #{n}:")))
        .unwrap_or_else(|| panic!("no merge of #{n}"))[..40]
        .to_owned();
    t.git(&[
        "log",
        "-1",
        &format!("--format=%(trailers:key={key},valueonly,separator= )"),
        &merge,
    ])
}

/// A stacked PR merges before the PR it carries, whatever their numbers: its merge brings the
/// base's commits, the base is `in`, and the todos both complete ride on that merge, each once.
/// The stack numbered upward and downward builds one tree, and so does a second build.
#[test]
fn train_build_merges_a_stack_by_ancestry_and_carries_what_it_completes() {
    let mut trees = Vec::new();
    for (base, top) in [(3, 5), (5, 3)] {
        let t = Train::new(&format!("stack-{base}-{top}"));
        t.pr(base, "the base", &[("a.txt", "a\n")]);
        std::fs::write(
            t.scratch.join(format!("prs/{base}.body")),
            "Completes: first\n",
        )
        .expect("a body");
        t.stacked(top, base, "the top", &[("b.txt", "b\n")]);
        std::fs::write(
            t.scratch.join(format!("prs/{top}.body")),
            "Completes: first, second\n",
        )
        .expect("a body");
        t.pr(4, "beside", &[("c.txt", "c\n")]);

        let out = t.train(&["build", "3", "4", "5"]);
        let stdout = text(&out.stdout);
        assert!(out.status.success(), "build: {}{stdout}", text(&out.stderr));
        let merges = t.merges();
        assert_eq!(merges.len(), 2, "one merge for the stack: {merges:?}");
        assert!(
            merges[0].starts_with(&format!("Merge #{top}:")),
            "the stack's top merges first: {merges:?}"
        );
        let row = stdout
            .lines()
            .find(|l| l.starts_with(&format!("#{base} ")))
            .unwrap_or_default();
        assert!(
            row.contains(" in ") && row.contains(&format!("carried by #{top}")),
            "the base is in: {stdout}"
        );
        assert_eq!(merge_trailer(&t, top, "Completes"), "first second");
        assert_eq!(merge_trailer(&t, top, "Carries"), base.to_string());
        let tree = t.git(&["rev-parse", "train^{tree}"]);

        let again = t.train(&["build", "5", "4", "3"]);
        assert!(again.status.success(), "build: {}", text(&again.stderr));
        assert_eq!(
            t.git(&["rev-parse", "train^{tree}"]),
            tree,
            "a second build of the set builds the same tree"
        );
        trees.push(tree);
    }
    assert_eq!(trees[0], trees[1], "the numbers move no tree");
}

/// PRs neither of which carries the other merge in the order of their heads' names, never of
/// their numbers.
#[test]
fn train_build_orders_prs_by_their_heads_not_their_numbers() {
    let t = Train::new("by-head");
    t.pr(1, "one", &[("a.txt", "a\n")]);
    t.pr(2, "two", &[("b.txt", "b\n")]);
    let (one, two) = (t.head(1), t.head(2));
    // #1 is given the greater head, so the numbers and the heads disagree.
    let (low, high) = if one < two { (one, two) } else { (two, one) };
    for (n, head) in [(1, &high), (2, &low)] {
        run_git(
            &t.author(),
            &[
                "push",
                "-q",
                "-f",
                "origin",
                &format!("{head}:refs/pull/{n}/head"),
            ],
        );
        t.names(n, &format!("head {}", &head[..7]), head);
    }
    let out = t.train(&["build", "1", "2"]);
    assert!(out.status.success(), "build: {}", text(&out.stderr));
    let merges = t.merges();
    assert!(
        merges[0].starts_with("Merge #2:") && merges[1].starts_with("Merge #1:"),
        "#2 holds the lesser head: {merges:?}"
    );
}

/// A line a PR wrote under Unreleased, which git put under a release that landed after the PR
/// branched, is moved back under Unreleased and the table says so; a line a PR wrote under a
/// released heading stays where it wrote it.
#[test]
fn train_build_moves_a_line_written_under_unreleased_back_there() {
    let t = Train::new("changelog-place");
    let before = changelog_with("an entry master holds");
    t.master_moves(&[("CHANGELOG.md", &before)]);
    let mine = before.replace(
        "- an entry master holds\n",
        "- an entry master holds\n- mine\n",
    );
    t.pr(1, "mine", &[("CHANGELOG.md", &mine)]);
    // A release lands after #1 branched, moving master's entry under its own heading.
    let released = CHANGELOG.replace(
        "## [Unreleased]\n\n",
        "## [Unreleased]\n\n## [0.2.0]\n\n- an entry master holds\n\n",
    );
    t.master_moves(&[("CHANGELOG.md", &released)]);
    let fixed = released.replace("- the first release", "- the first release, fixed");
    t.pr(2, "a fix", &[("CHANGELOG.md", &fixed)]);

    let out = t.train(&["build", "1", "2"]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "build: {}{stdout}", text(&out.stderr));
    let row = |n: &str| {
        stdout
            .lines()
            .find(|l| l.starts_with(n))
            .unwrap_or_default()
            .to_owned()
    };
    assert!(
        row("#1").contains("moved 1 CHANGELOG.md line under Unreleased"),
        "{stdout}"
    );
    assert!(!row("#2").contains("moved"), "{stdout}");
    let changelog = t.git(&["show", "train:CHANGELOG.md"]);
    let released_at = changelog.find("## [0.2.0]").expect("the release");
    let first_at = changelog.find("## [0.1.0]").expect("the first release");
    assert!(
        changelog[..released_at].contains("- mine"),
        "under Unreleased:\n{changelog}"
    );
    assert!(
        !changelog[released_at..].contains("- mine")
            && changelog[released_at..first_at].contains("- an entry master holds"),
        "the release keeps its own:\n{changelog}"
    );
    assert!(
        changelog[first_at..].contains("- the first release, fixed"),
        "a released line the PR rewrote stays:\n{changelog}"
    );
    assert_eq!(
        t.git(&["status", "--porcelain"]),
        "",
        "the checkout is left clean"
    );
}

/// A `refs/pull/<n>/head` that is not the head GitHub names for the PR refuses the build, naming
/// both commits; once the ref has caught up, the same build goes ahead.
#[test]
fn train_build_refuses_a_pull_ref_behind_the_pr() {
    let t = Train::new("stale-head");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    let fetched = t.head(1);
    let named = t.origin_master();
    t.names(1, "the first", &named);
    let out = t.train(&["build", "1"]);
    let said = text(&out.stderr);
    assert!(!out.status.success(), "a stale ref was built");
    assert!(
        said.contains(&fetched) && said.contains(&named),
        "both commits named: {said}"
    );
    assert_eq!(t.merges(), Vec::<String>::new(), "nothing merged");

    t.names(1, "the first", &fetched);
    let out = t.train(&["build", "1"]);
    assert!(out.status.success(), "build: {}", text(&out.stderr));
    assert_eq!(t.merges(), vec!["Merge #1: the first".to_owned()]);
}

/// A gate judges the train's commit in a worktree of its own: the operator switching the
/// checkout to a red branch while a half runs reaches neither half, the verdicts are the
/// train's, and the worktree is gone after, as it is after a train killed outright.
#[test]
fn train_gate_reads_only_the_tree_it_gates() {
    let t = Train::new("worktree");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    let worktrees = || {
        t.git(&["worktree", "list", "--porcelain"])
            .lines()
            .filter(|l| l.starts_with("worktree "))
            .count()
    };

    let (hold, mut said) = holding(&t, "hold");
    let hold_path = hold.to_str().expect("a UTF-8 path");
    let gating = t
        .command(&["gate"], &[("GATE_HOLD", hold_path)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("train.sh starts");
    let mut line = String::new();
    std::io::BufRead::read_line(&mut said, &mut line).expect("the fifo reads");
    assert_eq!(line, "up\n", "the tree half runs");
    // The operator moves the checkout to a branch both halves would fail.
    t.git(&["checkout", "-q", "-B", "aside", "origin/master"]);
    write(&t.work(), "broken", "x\n");
    write(&t.work(), "broken-live", "x\n");
    t.git(&["add", "-A"]);
    t.git(&["commit", "-q", "-m", "red"]);
    std::fs::write(&hold, "").expect("the held gate goes on");
    let out = gating.wait_with_output().expect("the train ends");
    let stderr = text(&out.stderr);
    assert!(out.status.success(), "gate: {stderr}");
    assert!(
        stderr.contains("tree half: ok") && stderr.contains("live half: ok"),
        "{stderr}"
    );
    let work = t.work().canonicalize().expect("the clone");
    for flag in ["--no-live", "--only-live"] {
        assert!(
            t.ran_in(flag).iter().all(|dir| *dir != work),
            "{flag} ran outside the checkout: {:?}",
            t.ran_in(flag)
        );
    }
    assert_eq!(worktrees(), 1, "the gate's worktree is gone");
    assert_eq!(
        t.git(&["branch", "--show-current"]),
        "aside",
        "the checkout is the operator's"
    );

    t.git(&["checkout", "-q", "train"]);
    let landed = t.train(&["land"]);
    assert!(landed.status.success(), "land: {}", text(&landed.stderr));

    // A train killed outright leaves its worktree; the next gate clears it and runs.
    t.pr(2, "the second", &[("b.txt", "b\n")]);
    assert!(t.train(&["build", "2"]).status.success());
    let mut gating = t
        .command(&["gate"], &[("GATE_HOLD", hold_path)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("train.sh starts");
    line.clear();
    std::io::BufRead::read_line(&mut said, &mut line).expect("the fifo reads");
    assert_eq!(line, "up\n", "the tree half runs");
    run(
        Command::new("kill").args(["-KILL", &gating.id().to_string()]),
        "kill -KILL the train",
    );
    gating.wait().expect("the train ends");
    assert_eq!(worktrees(), 2, "the killed gate's worktree is left");
    std::fs::write(&hold, "").expect("the orphaned gate goes on");
    let lock = t.work().join(".git/train/lock");
    assert!(
        within_twenty_seconds(|| Command::new("flock")
            .args(["-n"])
            .arg(&lock)
            .arg("true")
            .status()
            .is_ok_and(|s| s.success())),
        "the orphaned gate lets the lock go"
    );
    let again = t.train(&["gate"]);
    assert!(again.status.success(), "gate: {}", text(&again.stderr));
    assert_eq!(worktrees(), 1, "the leftover worktree is cleared");
}

/// A gate runs only inside the `.#triage` shell, and refuses a variable its steps read that the
/// caller exported and the shell does not set, naming each; one the shell sets is its own. The
/// gate is handed the data tree and none of what the train reads for itself.
#[test]
fn train_gate_refuses_an_environment_beside_the_triage_shell() {
    let t = Train::new("environment");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    let mut outside = t.command(&["gate"], &[]);
    outside.env_remove("SUZATARY_SHELL");
    let out = outside.output().expect("train.sh runs");
    assert!(!out.status.success(), "a gate ran outside the shell");
    assert!(
        text(&out.stderr).contains("inside the .#triage shell"),
        "{}",
        text(&out.stderr)
    );

    for command in ["gate", "bisect"] {
        let out = t.train_with(
            &[command],
            &[("CARGO_BUILD_JOBS", "3"), ("SUZATARY_BIN", "/elsewhere")],
        );
        let said = text(&out.stderr);
        assert!(
            !out.status.success(),
            "{command} took an inherited variable"
        );
        assert!(
            said.contains("CARGO_BUILD_JOBS") && said.contains("SUZATARY_BIN"),
            "each one named: {said}"
        );
    }
    assert_eq!(
        t.probes("--no-live") + t.probes("--only-live"),
        0,
        "no half ran"
    );

    // The checkout's instance link names the data tree, absolutely, for a worktree that holds none.
    write(&t.work(), "instance.toml", "# the data tree\n");
    let out = t.train_with(
        &["gate"],
        &[
            ("SUZATARY_SHELL_SETS", "CARGO_BUILD_JOBS"),
            ("CARGO_BUILD_JOBS", "3"),
            ("SUZATARY_GATE_SERIAL", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "a variable the shell sets is the shell's, and the serial order is the caller's: {}",
        text(&out.stderr)
    );
    let handed = std::fs::read_to_string(t.probes_file().with_extension("env")).expect("the env");
    assert!(handed.contains("SUZATARY_GATE_SERIAL=1\n"), "{handed}");
    let instance = t
        .work()
        .join("instance.toml")
        .canonicalize()
        .expect("the link");
    assert!(
        handed.contains(&format!("SUZATARY_INSTANCE={}\n", instance.display())),
        "{handed}"
    );
    assert!(
        !handed.contains("SUZATARY_TRAIN_BIN=") && !handed.contains("SUZATARY_TRAIN_GH="),
        "the train's own are not handed on: {handed}"
    );
}

#[test]
fn train_land_refuses_a_head_no_gate_has_passed() {
    let t = Train::new("land-ungated");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    t.pr(2, "the second", &[("broken", "x\n")]);
    let seed = t.origin_master();
    assert!(t.train(&["build", "1"]).status.success());

    let refused = t.train(&["land"]);
    assert!(!refused.status.success(), "an ungated train must not land");
    assert!(
        text(&refused.stderr).contains("gate first"),
        "{}",
        text(&refused.stderr)
    );
    assert_eq!(t.origin_master(), seed, "nothing landed");

    assert!(
        t.train(&["gate"]).status.success(),
        "the stub gate is green here"
    );
    let landed = t.train(&["land"]);
    assert!(landed.status.success(), "land: {}", text(&landed.stderr));
    assert_eq!(
        t.origin_master(),
        t.git(&["rev-parse", "train"]),
        "master is the train"
    );
    assert!(
        text(&landed.stdout).contains("scripts/deploy.sh"),
        "the next commands are printed"
    );

    assert!(t.train(&["build", "2"]).status.success());
    assert!(
        !t.train(&["gate"]).status.success(),
        "a tree holding `broken` is red"
    );
    let red = t.train(&["land"]);
    assert!(!red.status.success(), "a red train must not land");
    assert!(
        text(&red.stderr).contains("gate failed"),
        "{}",
        text(&red.stderr)
    );
}

#[test]
fn train_land_rebuilds_the_train_on_a_master_that_moved_and_lands_only_its_gated_tree() {
    let t = Train::new("land-moved");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let gated = t.git(&["rev-parse", "train"]);

    let refuse = suzatary_testkit::fake_program(
        &t.scratch.join("origin.git/hooks"),
        "pre-receive",
        "#!/bin/sh\nexit 1\n",
    );
    assert!(
        !t.train(&["land"]).status.success(),
        "origin refused the push"
    );
    assert_eq!(
        t.git(&["rev-parse", "train"]),
        gated,
        "a refused land leaves the train"
    );
    std::fs::remove_file(&refuse).expect("the refusal goes");

    // Files the train does not touch are still a tree no gate saw.
    t.master_moves(&[("c.txt", "c\n"), ("docs/notes.md", "notes\n")]);
    let moved = t.origin_master();
    let refused = t.train(&["land"]);
    assert!(
        !refused.status.success(),
        "a master that moved since the gate must not land"
    );
    assert!(
        text(&refused.stderr).contains("built again"),
        "{}",
        text(&refused.stderr)
    );
    assert_eq!(t.origin_master(), moved, "nothing landed");
    assert_eq!(
        t.git(&["merge-base", "train", "origin/master"]),
        moved,
        "the train is built again on the master that moved"
    );
    assert_eq!(t.merges(), ["Merge #1: the first"], "with the same PRs");

    let ungated = t.train(&["land"]);
    assert!(
        !ungated.status.success() && text(&ungated.stderr).contains("gate first"),
        "the rebuilt train is a new tree, so it is gated before it lands: {}",
        text(&ungated.stderr)
    );
    assert!(t.train(&["gate"]).status.success());
    let landed = t.train(&["land"]);
    assert!(landed.status.success(), "land: {}", text(&landed.stderr));
    let master = t.origin_master();
    assert_eq!(
        t.git(&["rev-parse", "train"]),
        master,
        "the gated train is what landed"
    );
    assert!(
        t.git(&["rev-list", "--first-parent", &master])
            .lines()
            .any(|rev| rev == moved),
        "master's previous tip stays on its first-parent chain, where a deploy looks for it"
    );
}

#[test]
fn train_land_on_a_master_that_moved_under_the_train_names_the_conflict() {
    let t = Train::new("land-under");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    t.master_moves(&[("a.txt", "another a\n")]);
    let moved = t.origin_master();

    let refused = t.train(&["land"]);
    let said = text(&refused.stderr);
    assert!(!refused.status.success(), "{said}");
    assert!(
        said.contains("#1") && said.contains("a.txt") && said.contains("no PR merged"),
        "the rebuild skips the PR that no longer merges, by name: {said}"
    );
    assert_eq!(t.origin_master(), moved, "nothing landed");
    assert_eq!(
        t.git(&["rev-parse", "train"]),
        moved,
        "the train holds no merge"
    );
}

#[test]
fn train_bisect_names_the_merge_that_broke_the_train() {
    let t = Train::new("bisect");
    for n in 1..=4 {
        let file = format!("f{n}.txt");
        let mut files = vec![(file.as_str(), "x\n")];
        if n == 3 {
            files.push(("broken", "x\n"));
        }
        t.pr(n, &format!("change {n}"), &files);
    }
    assert!(t.train(&["build", "1", "2", "3", "4"]).status.success());
    assert!(!t.train(&["gate"]).status.success(), "the train is red");
    assert_eq!(t.probes("--no-live"), 1);
    assert_eq!(t.probes("--only-live"), 1, "a gate runs both halves");
    // The PRs in the order the train merged them, by their heads.
    let order: Vec<String> = t
        .merges()
        .iter()
        .map(|m| m["Merge #".len()..m.find(':').expect("a subject")].to_owned())
        .collect();
    let broken = order.iter().position(|n| n == "3").expect("#3 merged");

    let out = t.train(&["bisect"]);
    let stdout = text(&out.stdout);
    assert!(
        out.status.success(),
        "bisect: {}{stdout}",
        text(&out.stderr)
    );
    let green: Vec<String> = order[..broken].iter().map(|n| format!("#{n}")).collect();
    assert!(
        stdout.contains(
            format!(
                "green in the tree half: master and the first {broken} merges {}",
                green.join(" ")
            )
            .trim_end()
        ),
        "{stdout}"
    );
    assert!(stdout.contains("broke it: Merge #3: change 3"), "{stdout}");
    let next: Vec<&str> = order
        .iter()
        .filter(|n| *n != "3")
        .map(String::as_str)
        .collect();
    assert!(
        stdout.contains(&format!("build {}", next.join(" "))),
        "the next train leaves #3 out: {stdout}"
    );
    assert_eq!(
        t.probes("--no-live"),
        3,
        "four merges, two probes of the red half"
    );
    assert_eq!(t.probes("--only-live"), 1, "the green half is not probed");

    assert!(t.train(&["bisect"]).status.success());
    assert_eq!(
        t.probes("--no-live"),
        3,
        "a prefix gated before is read from the table"
    );
    assert_eq!(
        t.git(&["branch", "--show-current"]),
        "train",
        "back on the train"
    );
}

#[test]
fn train_land_needs_both_halves_and_bisect_follows_the_red_one() {
    let t = Train::new("halves");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    t.pr(2, "the second", &[("broken-live", "x\n")]);
    let seed = t.origin_master();
    assert!(t.train(&["build", "1", "2"]).status.success());

    let red = t.train(&["gate"]);
    let stderr = text(&red.stderr);
    assert!(!red.status.success(), "a red live half is a red train");
    assert!(
        stderr.contains("tree half: ok") && stderr.contains("live half: failed"),
        "{stderr}"
    );
    assert!(
        stderr.contains("broken-live is in the tree"),
        "the failing steps' last lines: {stderr}"
    );
    let refused = t.train(&["land"]);
    assert!(
        !refused.status.success(),
        "the tree half alone does not land"
    );
    assert!(
        text(&refused.stderr).contains("live: failed"),
        "{}",
        text(&refused.stderr)
    );
    assert_eq!(t.origin_master(), seed, "nothing landed");

    let out = t.train(&["bisect"]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "bisect: {}", text(&out.stderr));
    assert!(stdout.contains("green in the live half"), "{stdout}");
    assert!(
        stdout.contains("broke it: Merge #2: the second"),
        "{stdout}"
    );
}

#[test]
fn train_gate_remote_runs_the_tree_half_in_the_codespace_and_stops_it() {
    let t = Train::new("remote");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    let out = t.train(&["gate", "--remote", "cs"]);
    assert!(out.status.success(), "gate: {}", text(&out.stderr));
    let codespace = t.scratch.join("codespace");
    assert_eq!(
        t.ran_in("--no-live"),
        [codespace.canonicalize().expect("the clone")],
        "the tree half ran in the Codespace"
    );
    let live = t.ran_in("--only-live");
    assert!(
        live.len() == 1 && live[0] != t.work().canonicalize().expect("the clone"),
        "the live half ran here, in a worktree of its own: {live:?}"
    );
    assert_eq!(
        t.codespace_calls().last().map(String::as_str),
        Some("stop cs"),
        "stopped after: {:?}",
        t.codespace_calls()
    );
    t.kept_awake_and_let_go();
    assert_eq!(t.train_refs_on_origin(), "", "the train's ref is deleted");

    let landed = t.train(&["land"]);
    assert!(landed.status.success(), "land: {}", text(&landed.stderr));
    assert_eq!(t.origin_master(), t.git(&["rev-parse", "train"]));
}

#[test]
fn train_gate_remote_red_blocks_the_land_with_its_last_lines() {
    let t = Train::new("remote-red");
    t.pr(1, "the first", &[("broken", "x\n")]);
    let seed = t.origin_master();
    assert!(t.train(&["build", "1"]).status.success());

    let red = t.train(&["gate", "--remote", "cs"]);
    let stderr = text(&red.stderr);
    assert!(!red.status.success(), "a red tree half is a red train");
    assert!(
        stderr.contains("tree half: failed") && stderr.contains("live half: ok"),
        "{stderr}"
    );
    assert!(
        stderr.contains("broken is in the tree"),
        "the remote failing steps' last lines: {stderr}"
    );
    assert_eq!(
        t.codespace_calls().last().map(String::as_str),
        Some("stop cs"),
        "stopped after a red gate too"
    );
    t.kept_awake_and_let_go();
    assert_eq!(t.train_refs_on_origin(), "", "the train's ref is deleted");

    let refused = t.train(&["land"]);
    assert!(
        !refused.status.success(),
        "the live half alone does not land"
    );
    assert!(
        text(&refused.stderr).contains("tree: failed"),
        "{}",
        text(&refused.stderr)
    );
    assert_eq!(t.origin_master(), seed, "nothing landed");
}

#[test]
fn train_gate_remote_that_drops_records_no_verdict() {
    let t = Train::new("remote-down");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    let out = t.train_with(&["gate", "--remote", "cs"], &[("GH_SSH_DOWN", "1")]);
    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "no tree verdict, no green: {stderr}");
    assert!(
        stderr.contains("did not finish") && stderr.contains("connection reset"),
        "{stderr}"
    );
    assert_eq!(
        t.codespace_calls().last().map(String::as_str),
        Some("stop cs"),
        "stopped after a dropped connection too"
    );
    t.kept_awake_and_let_go();
    assert_eq!(t.train_refs_on_origin(), "");

    let refused = t.train(&["land"]);
    assert!(
        text(&refused.stderr).contains("tree: unknown"),
        "a dropped connection is no verdict, so the train is ungated: {}",
        text(&refused.stderr)
    );

    assert!(
        t.train(&["gate", "--remote", "cs"]).status.success(),
        "the next gate, connected, is green: the drop left nothing behind"
    );
}

#[test]
fn train_gate_remote_outlives_a_dropped_poll_and_regates_a_commit_it_holds() {
    let t = Train::new("remote-poll");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    // Call 1 reads the Codespace's HEAD, 2 starts the gate, 3 is the first poll (the
    // keep-alive's calls are not counted).
    let out = t.train_with(&["gate", "--remote", "cs"], &[("GH_SSH_DROP_AT", "3")]);
    assert!(
        out.status.success(),
        "a poll lost to a drop is read again: {}",
        text(&out.stderr)
    );

    let again = t.train(&["gate", "--remote", "cs"]);
    assert!(
        again.status.success(),
        "the Codespace already holds the commit, so nothing is sent: {}",
        text(&again.stderr)
    );
    assert_eq!(t.probes("--no-live"), 2, "the tree half ran each time");
}

#[test]
fn train_gate_remote_killed_by_the_operator_lets_the_codespace_go() {
    let t = Train::new("remote-killed");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    // The tree half says `up` on this fifo once it runs, and the train's waiter says
    // `gone` once the train ends, so a train that dies first fails here, never hangs.
    let hold = t.scratch.join("hold");
    let up = hold.with_extension("up");
    for fifo in [&hold, &up] {
        run(Command::new("mkfifo").arg(fifo), "mkfifo");
    }
    let mut said = std::io::BufReader::new(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&up)
            .expect("the fifo opens"),
    );
    let hold_path = hold.to_str().expect("a UTF-8 path");
    let mut gating = t
        .command(&["gate", "--remote", "cs"], &[("GATE_HOLD", hold_path)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("train.sh starts");
    let pid = gating.id().to_string();
    let waiter = std::thread::spawn(move || {
        let status = gating.wait().expect("the train ends");
        std::fs::write(&up, "gone\n").expect("the waiter says so");
        status
    });
    let mut line = String::new();
    std::io::BufRead::read_line(&mut said, &mut line).expect("the fifo reads");
    assert_eq!(
        line, "up\n",
        "the tree half was running when the train was killed"
    );
    run(Command::new("kill").arg(&pid), "kill the train");
    assert!(
        !waiter.join().expect("the waiter").success(),
        "a killed train is not green"
    );

    // A poll already in flight may still log after the stop, so the stop is looked
    // for, not read last.
    assert!(
        t.codespace_calls().iter().any(|call| call == "stop cs"),
        "stopped after the operator's kill too: {:?}",
        t.codespace_calls()
    );
    t.kept_awake_and_let_go();
    let after = t.train(&["land"]);
    assert!(
        text(&after.stderr).contains("tree: unknown"),
        "the tree half's polls ended with the train, so its lock is free: {}",
        text(&after.stderr)
    );
    std::fs::write(&hold, "").expect("the held gate goes on");
}

/// A held gate's two fifos: `<name>`, which it waits on, and `<name>.up`, on which it says it
/// runs, opened read-write here so a gate that never comes up fails a read, never hangs it.
fn holding(t: &Train, name: &str) -> (PathBuf, std::io::BufReader<std::fs::File>) {
    let hold = t.scratch.join(name);
    let up = hold.with_extension("up");
    for fifo in [&hold, &up] {
        run(Command::new("mkfifo").arg(fifo), "mkfifo");
    }
    let said = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&up)
        .expect("the fifo opens");
    (hold, std::io::BufReader::new(said))
}

/// Polls `done` for twenty seconds.
fn within_twenty_seconds(done: impl Fn() -> bool) -> bool {
    (0..200).any(|_| {
        done() || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            false
        }
    })
}

/// The Codespace is let go when its half is done, while the live half here still runs: a
/// Codespace waiting on the box is billed for nothing.
#[test]
fn train_gate_remote_lets_the_codespace_go_before_the_live_half_ends() {
    let t = Train::new("remote-early");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    let (hold, mut said) = holding(&t, "hold-live");
    let hold_path = hold.to_str().expect("a UTF-8 path");
    let gating = t
        .command(
            &["gate", "--remote", "cs"],
            &[("GATE_HOLD_LIVE", hold_path)],
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("train.sh starts");
    let mut line = String::new();
    std::io::BufRead::read_line(&mut said, &mut line).expect("the fifo reads");
    assert_eq!(line, "up\n", "the live half runs");
    assert!(
        within_twenty_seconds(|| t.codespace_calls().iter().any(|call| call == "stop cs")),
        "the Codespace was stopped while the live half ran: {:?}",
        t.codespace_calls()
    );
    assert_eq!(
        t.keepers_running(),
        Vec::<String>::new(),
        "and its keep-alive with it"
    );
    std::fs::write(&hold, "").expect("the held live half goes on");
    let out = gating.wait_with_output().expect("the train ends");
    assert!(out.status.success(), "gate: {}", text(&out.stderr));
    assert_eq!(
        t.codespace_calls()
            .iter()
            .filter(|call| *call == "stop cs")
            .count(),
        1,
        "stopped once: {:?}",
        t.codespace_calls()
    );
}

/// A train killed with no chance to run its trap takes its keep-alive with it, which stops the
/// Codespace as the trap would have, so nothing holds it up or leaves it running.
#[test]
fn train_gate_remote_keep_alive_ends_with_a_train_killed_outright() {
    let t = Train::new("remote-sigkill");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    let (hold, mut said) = holding(&t, "hold");
    let hold_path = hold.to_str().expect("a UTF-8 path");
    let mut gating = t
        .command(&["gate", "--remote", "cs"], &[("GATE_HOLD", hold_path)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("train.sh starts");
    let mut line = String::new();
    std::io::BufRead::read_line(&mut said, &mut line).expect("the fifo reads");
    assert_eq!(line, "up\n", "the tree half runs");
    assert!(
        !t.keepers_running().is_empty(),
        "a keep-alive holds the Codespace up while the gate runs"
    );
    run(
        Command::new("kill").args(["-KILL", &gating.id().to_string()]),
        "kill -KILL the train",
    );
    gating.wait().expect("the train ends");
    assert!(
        within_twenty_seconds(|| t.keepers_running().is_empty()),
        "the keep-alive ended with the train: {:?} still run",
        t.keepers_running()
    );
    assert!(
        within_twenty_seconds(|| t.codespace_calls().iter().any(|call| call == "stop cs")),
        "and the Codespace was stopped: {:?}",
        t.codespace_calls()
    );
    std::fs::write(&hold, "").expect("the held gate goes on");
}

impl Train {
    /// Core as a GitHub repository, `github.com/test/core`, which git reaches in `core.git`, so
    /// the train derives the repository the stub `gh` answers for; and every ref pushed to core
    /// from now on logged, `<ref> <new sha>` a line, a deletion's sha all zeros.
    fn on_github(&self) {
        let url = "https://github.com/test/core.git";
        self.git(&["remote", "set-url", "core", url]);
        let core = self.scratch.join("core.git");
        self.git(&["config", &format!("url.{}.insteadOf", core.display()), url]);
        let _ = suzatary_testkit::fake_program(
            &core.join("hooks"),
            "update",
            &format!(
                "#!/bin/sh\necho \"$1 $3\" >>'{}'\n",
                self.scratch.join("core-pushes").display()
            ),
        );
    }

    /// The refs pushed to core, in order: `<ref> <new sha>`.
    fn core_pushes(&self) -> Vec<String> {
        std::fs::read_to_string(self.scratch.join("core-pushes"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The stub `gh`'s calls to GitHub's API.
    fn github_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.scratch.join("github.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The arguments of each run of the stub gate.
    fn gate_args(&self) -> Vec<String> {
        std::fs::read_to_string(self.probes_file().with_extension("args"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

const ZERO: &str = "0000000000000000000000000000000000000000";

/// `gate --remote github`: the box runs its steps of the tree half first, then the train commit's
/// public tree goes to core, committed on core's master by the publications' author with a `Gate:`
/// trailer, pushed to `gate/<train sha>` and to no other ref; the runner gates that commit alone,
/// its success is the half's green, and the branch is deleted once the run is read. The live half
/// runs on the box as ever, and the train lands.
#[test]
fn train_gate_github_runs_the_public_tree_on_a_gate_branch_and_records_its_green() {
    let t = Train::new("github");
    t.on_github();
    t.pr(
        1,
        "the first",
        &[("a.txt", "a\n"), ("private/notes.md", "private\n")],
    );
    assert!(t.train(&["build", "1"]).status.success());
    let train = t.git(&["rev-parse", "train"]);
    let master = t.core(&["rev-parse", "master"]);

    let out = t.train(&["gate", "--remote", "github"]);
    assert!(out.status.success(), "gate: {}", text(&out.stderr));
    assert!(
        t.gate_args().iter().any(
            |args| args.ends_with("--no-live --steps fmt,public-tree,deployed-shape,changelog")
        ),
        "the box ran its steps: {:?}",
        t.gate_args()
    );
    let refs = t.core_pushes();
    let branch = format!("refs/heads/gate/{train}");
    assert_eq!(refs.len(), 2, "one push and one deletion: {refs:?}");
    let pushed = refs[0]
        .strip_prefix(&format!("{branch} "))
        .unwrap_or_else(|| panic!("only gate/<train sha> is pushed: {refs:?}"))
        .to_owned();
    assert_eq!(refs[1], format!("{branch} {ZERO}"), "and then deleted");
    assert_eq!(
        t.core(&["rev-parse", "master"]),
        master,
        "core's master never moves"
    );
    assert_eq!(
        t.core(&["for-each-ref", "refs/heads/gate/"]),
        "",
        "no gate branch is left"
    );

    assert_eq!(t.core(&["rev-parse", &format!("{pushed}^")]), master);
    assert_eq!(
        t.core(&["log", "-1", "--format=%an <%ae>", &pushed]),
        "Suzatary <publish@suzatary.invalid>"
    );
    assert_eq!(
        t.core(&[
            "log",
            "-1",
            "--format=%(trailers:key=Gate,valueonly)",
            &pushed
        ]),
        train
    );
    let files = t.core(&["ls-tree", "-r", "--name-only", &pushed]);
    assert!(
        files.lines().any(|f| f == "a.txt") && !files.contains("private/"),
        "the train's public tree: {files}"
    );
    let runner = t.scratch.join(format!("runner-{pushed}"));
    assert!(
        t.ran_in("--no-live")
            .contains(&runner.canonicalize().expect("the runner's checkout")),
        "the runner gated the pushed commit"
    );
    assert!(
        t.github_calls()
            .iter()
            .any(|call| call.contains(&format!("head_sha={pushed}"))),
        "the train waited on that commit's run: {:?}",
        t.github_calls()
    );

    let landed = t.train(&["land"]);
    assert!(landed.status.success(), "land: {}", text(&landed.stderr));
    assert_eq!(t.origin_master(), train);
}

/// A run whose gate failed is the tree half's red, with the gate's failing lines, read off the
/// job's log, and the land is refused; the branch is deleted all the same.
#[test]
fn train_gate_github_red_run_is_the_halfs_red() {
    let t = Train::new("github-red");
    t.on_github();
    t.pr(1, "the first", &[("broken", "x\n")]);
    let seed = t.origin_master();
    assert!(t.train(&["build", "1"]).status.success());

    let red = t.train(&["gate", "--remote", "github"]);
    let stderr = text(&red.stderr);
    assert!(!red.status.success());
    assert!(
        stderr.contains("tree half: failed") && stderr.contains("live half: ok"),
        "{stderr}"
    );
    assert!(
        stderr.contains("broken is in the tree") && stderr.contains("red: gate (tests)"),
        "the job's failing lines: {stderr}"
    );
    assert_eq!(t.core(&["for-each-ref", "refs/heads/gate/"]), "");
    let refused = t.train(&["land"]);
    assert!(text(&refused.stderr).contains("tree: failed"));
    assert_eq!(t.origin_master(), seed, "nothing landed");
}

/// A run the cloud ended (cancelled, or failed with no gate red, as a lost runner's) says nothing
/// of the tree: no verdict, so the train lands nothing, and the next gate pushes a commit of its
/// own and is read afresh.
#[test]
fn train_gate_github_cancelled_or_lost_run_is_no_verdict() {
    for how in ["cancelled", "lost"] {
        let t = Train::new(&format!("github-{how}"));
        t.on_github();
        t.pr(1, "the first", &[("a.txt", "a\n")]);
        assert!(t.train(&["build", "1"]).status.success());

        let out = t.train_with(&["gate", "--remote", "github"], &[("GH_RUN", how)]);
        let stderr = text(&out.stderr);
        assert!(
            !out.status.success(),
            "{how}: no tree verdict, no green: {stderr}"
        );
        assert!(
            stderr.contains("tree half: unknown") && stderr.contains("says nothing of"),
            "{how}: {stderr}"
        );
        assert_eq!(t.core(&["for-each-ref", "refs/heads/gate/"]), "");
        assert!(
            text(&t.train(&["land"]).stderr).contains("tree: unknown"),
            "{how}: nothing to land on"
        );

        let again = t.train(&["gate", "--remote", "github"]);
        assert!(
            again.status.success(),
            "{how}: the next gate is green: {}",
            text(&again.stderr)
        );
        let pushed: BTreeSet<String> = t
            .core_pushes()
            .iter()
            .filter(|line| !line.ends_with(ZERO))
            .map(|line| line.split(' ').nth(1).expect("a sha").to_owned())
            .collect();
        assert_eq!(
            pushed.len(),
            2,
            "{how}: each gate pushed a commit of its own"
        );
    }
}

/// The box's steps red: the half is red, and nothing goes to core.
#[test]
fn train_gate_github_red_on_the_box_pushes_nothing() {
    let t = Train::new("github-box-red");
    t.on_github();
    t.pr(1, "the first", &[("broken-box", "x\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    let out = t.train(&["gate", "--remote", "github"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("tree half: failed"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(t.core_pushes(), Vec::<String>::new(), "nothing pushed");
    assert_eq!(t.github_calls(), Vec::<String>::new(), "no run asked for");
}

/// A public tree its leak guard refuses never leaves the box: nothing is pushed, no step runs, and
/// the tree half says nothing of the tree.
#[test]
fn train_gate_github_whose_public_tree_leaks_pushes_nothing() {
    let t = Train::new("github-leak");
    t.on_github();
    t.pr(1, "the first", &[("a.txt", "a leak\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    let out = t.train(&["gate", "--remote", "github"]);
    let stderr = text(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("leak guard") && stderr.contains("tree half: unknown"),
        "{stderr}"
    );
    assert_eq!(t.core_pushes(), Vec::<String>::new(), "nothing pushed");
    assert_eq!(t.probes("--no-live"), 0, "no step of the tree half ran");
}

/// A train killed while it waits on the cloud's run deletes its gate branch on the way out.
#[test]
fn train_gate_github_killed_deletes_its_branch() {
    let t = Train::new("github-killed");
    t.on_github();
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    let train = t.git(&["rev-parse", "train"]);

    let gating = t
        .command(&["gate", "--remote", "github"], &[("GH_RUN", "running")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("train.sh starts");
    let branch = format!("refs/heads/gate/{train}");
    assert!(
        within_twenty_seconds(|| {
            t.core(&["for-each-ref", "--format=%(refname)", "refs/heads/gate/"]) == branch
                && t.github_calls()
                    .iter()
                    .any(|call| call.contains("/actions/runs/") && !call.contains("head_sha"))
        }),
        "the train pushed its branch and waits on the run: {:?}",
        t.github_calls()
    );
    run(
        Command::new("kill").arg(gating.id().to_string()),
        "kill the train",
    );
    let out = gating.wait_with_output().expect("the train ends");
    assert!(!out.status.success(), "a killed train is not green");
    assert_eq!(
        t.core(&["for-each-ref", "refs/heads/gate/"]),
        "",
        "its gate branch went with it"
    );
}

/// A land publishes the landed commit's public tree to core's master as one commit naming its
/// source, on the commit before it; publishing it again is that same row, so nothing moves.
#[test]
fn train_land_publishes_the_landed_tree_once() {
    let t = Train::new("publish");
    t.pr(
        1,
        "the first",
        &[("a.txt", "a\n"), ("private/notes.md", "private\n")],
    );
    let populated = t.core(&["rev-parse", "master"]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let landed = t.train(&["land"]);
    assert!(landed.status.success(), "land: {}", text(&landed.stderr));
    let master = t.origin_master();
    assert!(
        text(&landed.stdout).contains(&format!("scripts/deploy.sh {master}")),
        "{}",
        text(&landed.stdout)
    );

    let published = t.core(&["rev-parse", "master"]);
    assert_eq!(
        t.core(&["rev-parse", "master^"]),
        populated,
        "one commit on the one before"
    );
    assert_eq!(
        t.core(&[
            "log",
            "-1",
            "--format=%(trailers:key=Source,valueonly,separator=%x2C)",
            "master"
        ]),
        master
    );
    assert_eq!(t.core(&["show", "master:a.txt"]), "a");
    assert!(
        t.core(&["ls-tree", "-r", "--name-only", "master"])
            .lines()
            .all(|path| !path.starts_with("private/")),
        "the private history is not published"
    );
    assert_eq!(
        t.core(&["log", "-1", "--format=%an <%ae>", "master"]),
        "Suzatary <publish@suzatary.invalid>",
        "no person's name rides a publication"
    );

    let again = t.train(&["publish"]);
    assert!(again.status.success(), "{}", text(&again.stderr));
    assert!(
        text(&again.stderr).contains("already publishes"),
        "{}",
        text(&again.stderr)
    );
    assert_eq!(
        t.core(&["rev-parse", "master"]),
        published,
        "a source is published once"
    );
}

/// A public tree its leak guard refuses is not published, while master stays landed, and land
/// says both.
#[test]
fn train_land_whose_public_tree_leaks_lands_and_publishes_nothing() {
    let t = Train::new("publish-leak");
    t.pr(1, "the first", &[("a.txt", "a leak\n")]);
    let populated = t.core(&["rev-parse", "master"]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let landed = t.train(&["land"]);
    assert!(
        !landed.status.success(),
        "a land it could not publish fails"
    );
    let said = text(&landed.stderr);
    assert!(said.contains("Nothing published"), "{said}");
    assert!(said.contains("master landed at"), "{said}");
    assert!(said.contains("a.txt"), "the guard names the file: {said}");
    assert_eq!(
        t.origin_master(),
        t.git(&["rev-parse", "train"]),
        "master stays landed"
    );
    assert_eq!(
        t.core(&["rev-parse", "master"]),
        populated,
        "core did not move"
    );
}

/// Only a commit master's first-parent chain took is published, and the table only climbs: a
/// source older than the newest published, or one a merge only brought, is refused.
#[test]
fn train_publish_only_climbs_master_first_parent_chain() {
    let t = Train::new("publish-climbs");
    let seed = t.origin_master();
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    let series = run_git(&t.author(), &["rev-parse", "HEAD"]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());
    let published = t.core(&["rev-parse", "master"]);

    let older = t.train(&["publish", &seed]);
    assert!(!older.status.success());
    assert!(
        text(&older.stderr).contains("only climbs"),
        "{}",
        text(&older.stderr)
    );
    let side = t.train(&["publish", &series]);
    assert!(!side.status.success());
    assert!(
        text(&side.stderr).contains("first-parent"),
        "{}",
        text(&side.stderr)
    );
    assert_eq!(
        t.core(&["rev-parse", "master"]),
        published,
        "core did not move"
    );
}

/// A land that could not reach core loses nothing: the next publish walks every landed commit
/// after the newest published, oldest first, one core commit each. (Where core publishes
/// nothing yet, the first publish is the one commit named, never the whole history.)
#[test]
fn train_publish_catches_up_every_landed_commit_in_order() {
    let t = Train::new("publish-catch-up");
    t.pr(1, "the start", &[("start.txt", "s\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());
    let populated = t.core(&["rev-parse", "master"]);

    t.git(&["remote", "set-url", "core", "../nowhere.git"]);
    t.pr(3, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "3"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let unreached = t.train(&["land"]);
    assert!(!unreached.status.success());
    assert!(
        text(&unreached.stderr).contains("could not reach core"),
        "{}",
        text(&unreached.stderr)
    );
    let first = t.origin_master();

    t.git(&["remote", "set-url", "core", "../core.git"]);
    t.pr(4, "the second", &[("b.txt", "b\n")]);
    assert!(t.train(&["build", "4"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let landed = t.train(&["land"]);
    assert!(landed.status.success(), "land: {}", text(&landed.stderr));
    let second = t.origin_master();
    assert_eq!(
        t.core(&[
            "log",
            "--reverse",
            "--format=%(trailers:key=Source,valueonly,separator=%x2C)",
            &format!("{populated}..master"),
        ]),
        format!("{first}\n{second}"),
        "one commit per landed commit, oldest first"
    );
    assert_eq!(t.core(&["show", "master~1:a.txt"]), "a");
}

/// A commit step that names no commit stops the publish before the push, which with an empty
/// name would delete core's master.
#[test]
fn train_publish_refuses_to_push_an_empty_commit() {
    let t = Train::new("publish-empty");
    let populated = t.core(&["rev-parse", "master"]);
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());
    let published = t.core(&["rev-parse", "master"]);
    assert_ne!(published, populated);

    t.pr(2, "the second", &[("b.txt", "b\n")]);
    assert!(t.train(&["build", "2"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let real = run(
        Command::new("sh").args(["-c", "command -v git"]),
        "which git",
    );
    let broken = t.scratch.join("broken");
    let _ = suzatary_testkit::fake_program(
        &broken,
        "git",
        &format!("#!/bin/sh\n[ \"$1\" = commit-tree ] && exit 0\nexec {real} \"$@\"\n"),
    );
    let path = format!(
        "{}:{}",
        broken.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let refused = t.train_with(&["land"], &[("PATH", &path)]);
    assert!(!refused.status.success());
    assert!(
        text(&refused.stderr).contains("named no commit"),
        "{}",
        text(&refused.stderr)
    );
    assert_eq!(
        t.core(&["rev-parse", "master"]),
        published,
        "core's master is neither moved nor deleted"
    );
}

/// An empty <rev> is refused, never read as master's tip: a variable that read nothing upstream
/// would otherwise publish the tip and leave the rev meant below the table for good.
#[test]
fn train_publish_refuses_an_empty_rev() {
    let t = Train::new("publish-empty-rev");
    let populated = t.core(&["rev-parse", "master"]);
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    t.git(&["remote", "set-url", "core", "../nowhere.git"]);
    assert!(
        !t.train(&["land"]).status.success(),
        "landed, not published"
    );
    t.git(&["remote", "set-url", "core", "../core.git"]);

    let refused = t.train(&["publish", ""]);
    assert!(!refused.status.success());
    assert!(
        text(&refused.stderr).contains("empty <rev>"),
        "{}",
        text(&refused.stderr)
    );
    assert_eq!(
        t.core(&["rev-parse", "master"]),
        populated,
        "nothing published"
    );
}

/// A landed commit whose tree can never pass blocks every row after it, naming its file and line
/// and never the term; `publish --skip <sha>` passes over it, only once its own check fails, and
/// the next row names it in a `Skipped:` trailer beside its own `Source:`. Publishing it after
/// is refused as skipped.
#[test]
fn train_publish_skips_a_commit_only_when_it_fails() {
    let t = Train::new("publish-skip");
    t.pr(1, "the start", &[("start.txt", "s\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());
    let published = t.core(&["rev-parse", "master"]);

    t.pr(3, "the leak", &[("a.txt", "a leak\n")]);
    assert!(t.train(&["build", "3"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let blocked = t.train(&["land"]);
    assert!(!blocked.status.success());
    assert!(
        text(&blocked.stderr).contains("a.txt"),
        "{}",
        text(&blocked.stderr)
    );
    let leaked = t.origin_master();
    assert!(
        text(&blocked.stderr).contains(&format!("publish --skip {leaked}")),
        "{}",
        text(&blocked.stderr)
    );

    t.pr(4, "the mend", &[("a.txt", "mended\n")]);
    assert!(t.train(&["build", "4"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(
        !t.train(&["land"]).status.success(),
        "the leaking commit blocks the rows after it"
    );
    assert_eq!(t.core(&["rev-parse", "master"]), published);
    let mended = t.origin_master();

    let last = t.train(&["publish", "--skip", &mended]);
    assert!(
        !last.status.success(),
        "no row after the tip records its skip"
    );
    let skipped = t.train(&["publish", "--skip", &leaked]);
    assert!(skipped.status.success(), "{}", text(&skipped.stderr));
    assert_eq!(t.core(&["rev-parse", "master^"]), published, "one row");
    assert_eq!(
        t.core(&[
            "log",
            "-1",
            "--format=%(trailers:key=Source,valueonly)%(trailers:key=Skipped,valueonly)",
            "master"
        ]),
        format!("{mended}\n{leaked}")
    );
    assert_eq!(t.core(&["show", "master:a.txt"]), "mended");
    let again = t.train(&["publish", &leaked]);
    assert!(!again.status.success());
    assert!(
        text(&again.stderr).contains("was skipped by publish"),
        "{}",
        text(&again.stderr)
    );
}

/// A skip of a commit whose tree passes is refused before any row is pushed: a skipped commit is
/// never deployed, and deploy would call it a leak when it is not.
#[test]
fn train_publish_refuses_to_skip_a_clean_commit() {
    let t = Train::new("publish-skip-clean");
    t.pr(1, "the start", &[("start.txt", "s\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());
    let published = t.core(&["rev-parse", "master"]);

    t.git(&["remote", "set-url", "core", "../nowhere.git"]);
    t.pr(3, "clean", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "3"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(
        !t.train(&["land"]).status.success(),
        "landed, not published"
    );
    let clean = t.origin_master();
    t.pr(4, "after", &[("b.txt", "b\n")]);
    assert!(t.train(&["build", "4"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(
        !t.train(&["land"]).status.success(),
        "landed, not published"
    );
    t.git(&["remote", "set-url", "core", "../core.git"]);

    let refused = t.train(&["publish", "--skip", &clean]);
    assert!(!refused.status.success());
    assert!(
        text(&refused.stderr).contains(&format!(
            "{clean} passes its leak guard; publish it instead"
        )),
        "{}",
        text(&refused.stderr)
    );
    assert_eq!(
        t.core(&["rev-parse", "master"]),
        published,
        "core did not move"
    );
}

/// An admission no publication has held is held as a new path is: a line of the admitted files
/// (the file's path is known, but a person has not looked at these bytes of it), a fingerprint
/// of `.gitleaksignore`, a change to the secret rules. A land publishes nothing until `--admit`
/// names it at a terminal; a row that leaves the admissions as they were is published.
#[test]
#[allow(clippy::too_many_lines)]
fn train_publish_holds_a_new_admitted_line_until_a_person_admits_it() {
    let t = Train::new("publish-admitted-line");
    let manifest = |digest: &str| format!("# admitted\n{digest}  blob.txt\n");
    t.pr(
        1,
        "the start",
        &[
            ("blob.txt", "the first bytes\n"),
            ("notes.txt", "a note\n"),
            ("scripts/public-admitted.sha256", &manifest(&"a".repeat(64))),
            (".gitleaksignore", "# vouched\n"),
            ("scripts/secret-rules.toml", "[extend]\nuseDefault = true\n"),
        ],
    );
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(!t.train(&["land"]).status.success(), "five new paths");
    let first = t.at_a_terminal(&[
        "publish",
        "--admit",
        "blob.txt",
        "--admit",
        "notes.txt",
        "--admit",
        "scripts/public-admitted.sha256",
        "--admit",
        ".gitleaksignore",
        "--admit",
        "scripts/secret-rules.toml",
    ]);
    assert!(first.status.success(), "{}", text(&first.stdout));
    let published = t.core(&["rev-parse", "master"]);

    t.pr(
        2,
        "other bytes",
        &[
            ("blob.txt", "other bytes\n"),
            ("scripts/public-admitted.sha256", &manifest(&"b".repeat(64))),
        ],
    );
    assert!(t.train(&["build", "2"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let landed = t.train(&["land"]);
    assert!(!landed.status.success(), "a new admitted line is held");
    assert!(
        text(&landed.stderr).contains("held: blob.txt")
            && text(&landed.stderr).contains("--admit blob.txt"),
        "{}",
        text(&landed.stderr)
    );
    assert_eq!(
        t.core(&["rev-parse", "master"]),
        published,
        "core did not move"
    );

    let at_a_terminal = t.at_a_terminal(&["publish", "--admit", "blob.txt"]);
    assert!(
        at_a_terminal.status.success(),
        "{}",
        text(&at_a_terminal.stdout)
    );
    assert_eq!(
        t.core(&[
            "log",
            "-1",
            "--format=%(trailers:key=Admitted,valueonly)",
            "master"
        ]),
        "blob.txt"
    );

    t.pr(3, "a note", &[("notes.txt", "another note\n")]);
    assert!(t.train(&["build", "3"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let again = t.train(&["land"]);
    assert!(
        again.status.success(),
        "the admitted lines stand: {}",
        text(&again.stderr)
    );

    // A new fingerprint is held at the path it names, and a change to the rules at theirs.
    for (n, file, content, held) in [
        (
            4,
            ".gitleaksignore",
            "# vouched\nnotes.txt:generic-api-key:1\n",
            "notes.txt",
        ),
        (
            5,
            "scripts/secret-rules.toml",
            "[extend]\nuseDefault = false\n",
            "scripts/secret-rules.toml",
        ),
    ] {
        t.pr(n, "an admission", &[(file, content)]);
        assert!(t.train(&["build", &n.to_string()]).status.success());
        assert!(t.train(&["gate"]).status.success());
        let landed = t.train(&["land"]);
        assert!(!landed.status.success(), "{file}: held");
        assert!(
            text(&landed.stderr).contains(&format!("held: {held}")),
            "{file}: {}",
            text(&landed.stderr)
        );
        let admitted = t.at_a_terminal(&["publish", "--admit", held]);
        assert!(admitted.status.success(), "{}", text(&admitted.stdout));
    }
}

/// A path no publication has held is held: a land, unattended, publishes nothing holding it and
/// names it, with every later row's; `--admit` is refused away from a terminal, and for a path
/// no row holds; at one, each row that first holds a path is published with an `Admitted:`
/// trailer, and the path is known from then on.
#[test]
fn train_publish_holds_a_new_path_until_a_person_admits_it() {
    let t = Train::new("publish-admit");
    t.pr(1, "the start", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());
    let published = t.core(&["rev-parse", "master"]);
    let unmoved = || {
        assert_eq!(
            t.core(&["rev-parse", "master"]),
            published,
            "core did not move"
        );
    };

    t.pr(2, "a new file", &[("fresh.txt", "new\n")]);
    assert!(t.train(&["build", "2"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let landed = t.train(&["land"]);
    assert!(
        !landed.status.success(),
        "a land holding a new path publishes nothing"
    );
    assert!(
        text(&landed.stderr).contains("held: fresh.txt"),
        "{}",
        text(&landed.stderr)
    );
    assert!(
        text(&landed.stderr).contains("--admit fresh.txt"),
        "{}",
        text(&landed.stderr)
    );
    unmoved();

    t.pr(4, "another new file", &[("other.txt", "other\n")]);
    assert!(t.train(&["build", "4"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let both = t.train(&["land"]);
    assert!(!both.status.success(), "still held");
    for path in [
        "held: fresh.txt",
        "held: other.txt",
        "--admit fresh.txt --admit other.txt",
    ] {
        assert!(
            text(&both.stderr).contains(path),
            "every held row's paths are named: {}",
            text(&both.stderr)
        );
    }

    let scripted = t.train(&["publish", "--admit", "fresh.txt"]);
    assert!(
        !scripted.status.success(),
        "--admit from a script is refused"
    );
    assert!(
        text(&scripted.stderr).contains("terminal"),
        "{}",
        text(&scripted.stderr)
    );
    unmoved();

    let unheld = t.at_a_terminal(&["publish", "--admit", "fresh.txt", "--admit", "nothere.txt"]);
    assert!(
        !unheld.status.success(),
        "an admit of a path no row holds is refused"
    );
    assert!(
        text(&unheld.stdout).contains("--admit nothere.txt names no path"),
        "{}",
        text(&unheld.stdout)
    );
    unmoved();

    let at_a_terminal =
        t.at_a_terminal(&["publish", "--admit", "fresh.txt", "--admit", "other.txt"]);
    assert!(
        at_a_terminal.status.success(),
        "{}",
        text(&at_a_terminal.stdout)
    );
    assert_eq!(t.core(&["show", "master:fresh.txt"]), "new");
    let admitted = |rev: &str| {
        t.core(&[
            "log",
            "-1",
            "--format=%(trailers:key=Admitted,valueonly,separator=%x2C)",
            rev,
        ])
    };
    assert_eq!(admitted("master~1"), "fresh.txt");
    assert_eq!(admitted("master"), "other.txt");

    t.pr(3, "the same file again", &[("fresh.txt", "newer\n")]);
    assert!(t.train(&["build", "3"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    let again = t.train(&["land"]);
    assert!(
        again.status.success(),
        "a known path is published: {}",
        text(&again.stderr)
    );
}

/// A skip is taken only on a check that ran and found hits: where the public tree could not be
/// written, or the check could not run whole, the publish stops naming that, and core does not
/// move, whatever the tree holds.
#[test]
fn train_publish_never_skips_on_a_check_that_could_not_run() {
    let t = Train::new("publish-skip-unchecked");
    t.pr(1, "the start", &[("start.txt", "s\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());
    let published = t.core(&["rev-parse", "master"]);

    t.git(&["remote", "set-url", "core", "../nowhere.git"]);
    t.pr(3, "clean", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "3"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(
        !t.train(&["land"]).status.success(),
        "landed, not published"
    );
    let clean = t.origin_master();
    t.pr(4, "after", &[("b.txt", "b\n")]);
    assert!(t.train(&["build", "4"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(
        !t.train(&["land"]).status.success(),
        "landed, not published"
    );
    t.git(&["remote", "set-url", "core", "../core.git"]);

    for (flag, said) in [
        ("PUBLIC_UNWRITTEN", "could not write the public tree"),
        ("PUBLIC_UNCHECKED", "the leak guard could not run"),
    ] {
        let refused = t.train_with(&["publish", "--skip", &clean], &[(flag, "1")]);
        assert!(!refused.status.success(), "{flag}");
        assert!(
            text(&refused.stderr).contains(said),
            "{flag}: {}",
            text(&refused.stderr)
        );
        assert_eq!(
            t.core(&["rev-parse", "master"]),
            published,
            "{flag}: core did not move"
        );
        let row = t.train_with(&["publish"], &[(flag, "1")]);
        assert!(!row.status.success(), "{flag}");
        assert!(
            text(&row.stderr).contains(said) && !text(&row.stderr).contains("did not pass"),
            "{flag}: a row that could not be checked is not called a leak: {}",
            text(&row.stderr)
        );
    }
}

/// The project chain a release reads, under `data`: `suzatarytwo` composing three items, one
/// done; and the instance naming it, as `SUZATARY_INSTANCE` names the box's.
fn a_release_deployment(data: &Path) -> PathBuf {
    use prodrome_core::fpl;
    use suzatary_core::record::{EventStore, GENESIS, MarkupSource, mk_authored};
    let at = prodrome_core::literal::Datetime::new(2026, 9, 1, 9, 0, 0, 0).expect("an instant");
    let store = EventStore::new(
        data.join("events-suzatary"),
        prodrome_core::policy::Untrusted::none(),
    );
    let todo = |id: &str, spec| {
        mk_authored(
            id,
            at,
            "ada",
            "todo",
            at,
            MarkupSource::escaped(&format!("the {id} item")),
            spec,
            vec!["a fixture".to_owned()],
            "project",
            "",
            MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("a record")
    };
    let items = ["alpha", "bravo", "calendarlinks"];
    let mut terms = vec![fpl::mk_flat(0.6).expect("a term")];
    terms.extend(
        items
            .iter()
            .map(|i| fpl::mk_ref((*i).to_owned()).expect("a ref")),
    );
    let mut events = vec![todo(
        "suzatarytwo",
        fpl::mk_conj(terms, -4.0).expect("a conjunction"),
    )];
    events.extend(
        items
            .iter()
            .map(|i| todo(i, fpl::mk_flat(0.7).expect("a term"))),
    );
    events.push(prodrome_core::event::mk_completed("alpha", at, "ada", "").expect("an event"));
    for event in events {
        suzatary_core::store::append_to(&store, GENESIS, event).expect("appends");
    }
    let path = data.join("instance.toml");
    std::fs::write(
        &path,
        format!(
            r#"repo = "{data}"
[signatures]
root = "{key}"
[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["assistant", "ada", "triage"]
owner = "ada"
[[chains]]
name = "suzatary"
events = "events-suzatary"
untrusted = ["triage"]
actors = ["assistant", "ada", "triage"]
owner = "ada"
[web]
viewers = ["someone@example.com"]
app_url = "https://example.invalid"
[web.act_as]
"someone@example.com" = "ada"
[web.serving]
host = "127.0.0.1"
port = 8790
tls_dir = "tls"
dist = "dist"
briefings = "briefings"
[categories]
project = 0
[mail]
root = "mail"
state = "state.json"
notmuch = "notmuch"
mbsyncrc = "mbsyncrc"
heartbeat = "triage-heartbeat"
[[mail.accounts]]
email = "someone@example.com"
subdir = "someone"
sync = "gmail"
"#,
            data = data.display(),
            key = suzatary_core::cast::keys::root().as_str(),
        ),
    )
    .expect("an instance file");
    path
}

/// What a train run needs to read and write the fixture's project chain.
fn release_env(instance: &Path) -> Vec<(&'static str, String)> {
    let data = instance.parent().expect("the instance's directory");
    for dir in ["home", "run"] {
        std::fs::create_dir_all(data.join(dir)).expect("a scratch directory");
    }
    vec![
        ("SUZATARY_INSTANCE", instance.display().to_string()),
        // A login session's, as the box's operator has: the binary's paths live under them.
        ("HOME", data.join("home").display().to_string()),
        ("XDG_RUNTIME_DIR", data.join("run").display().to_string()),
        (
            suzatary_core::signatures::BOX_KEY,
            suzatary_core::cast::keys::ROOT.to_owned(),
        ),
        (
            suzatary_core::signatures::AGENT_KEY,
            suzatary_core::cast::keys::AGENT.to_owned(),
        ),
        (
            suzatary_core::mail::dedup_key::DEDUP_KEY,
            suzatary_core::cast::keys::DEDUP.to_owned(),
        ),
    ]
}

/// Every path a crate's source includes (`include_str!`, `include_bytes!`, `include!` of a
/// literal), resolved from the file that includes it.
fn included(dir: &Path, into: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir)
        .expect("a readable directory")
        .flatten()
    {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "target" && name != "node_modules" {
                included(&path, into);
            }
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a readable source");
        for macro_ in ["include_str!", "include_bytes!", "include!"] {
            for (at, _) in text.match_indices(macro_) {
                let rest = text[at + macro_.len()..].trim_start();
                let Some(rest) = rest.strip_prefix('(').map(str::trim_start) else {
                    continue;
                };
                let Some(literal) = rest.strip_prefix('"').and_then(|r| r.split('"').next()) else {
                    continue;
                };
                let from = path.parent().expect("a file's directory").join(literal);
                into.push(std::fs::canonicalize(&from).unwrap_or(from));
            }
        }
    }
}

/// THE VERSION IS NO COMPILED INPUT: what a release cut writes (the changelog and
/// `app/release.toml`, pinned by `train_releases_what_the_chain_reads_and_tags_it`) is
/// included by no crate's source, and no crate has a build script that could read it, so a
/// train's release moves no crate's inputs and no gate key. The binary reads its release at run
/// time, from the file the flake installs beside it (`suzatary_cli::stamp`).
#[test]
fn a_release_cut_changes_no_file_a_crate_compiles() {
    let app = repo().join("app");
    let cut: Vec<PathBuf> = ["CHANGELOG.md", "app/release.toml"]
        .iter()
        .map(|p| repo().join(p))
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    let mut paths = Vec::new();
    included(&app, &mut paths);
    assert!(
        paths.iter().any(|p| p.ends_with("app/core/releases.toml")),
        "the scan finds the plan the core includes: {paths:?}"
    );
    for path in &paths {
        assert!(!cut.contains(path), "a crate includes {}", path.display());
    }
    for crate_ in std::fs::read_dir(&app).expect("app/").flatten() {
        assert!(
            !crate_.path().join("build.rs").exists(),
            "{} has a build script, which could read the release",
            crate_.path().display()
        );
    }
}

/// A tree that declares a release plan, at `version`.
fn release_files(version: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "app/Cargo.toml",
            "[workspace.package]\nversion = \"0.0.0\"\n".to_owned(),
        ),
        ("app/release.toml", format!("version = \"{version}\"\n")),
        (
            "app/core/releases.toml",
            "chain = \"suzatary\"\nbase = 1\n\n[[release]]\ntodo = \"alpha\"\nmajor = 1\nminor = 1\n\n\
             [[release]]\ntodo = \"calendarlinks\"\nmajor = 1\nminor = 2\n\n\
             [[release]]\ntodo = \"suzatarytwo\"\nmajor = 2\nminor = 0\n"
                .to_owned(),
        ),
    ]
}

/// The todos a PR names complete are recorded done when it lands, and the train's release is
/// the version the chain then reads: cut at build so the gate judges it, checked against the
/// chain at land, and pushed with its tag in one push. The next train that completes nothing
/// is the next patch.
#[test]
fn train_releases_what_the_chain_reads_and_tags_it() {
    let t = Train::new("release");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_moves(&files);

    t.pr(
        1,
        "calendar links",
        &[("CHANGELOG.md", &changelog_with("calendar links"))],
    );
    std::fs::write(
        t.scratch.join("prs/1.body"),
        "Links in the calendar.\n\nCompletes: calendarlinks\n",
    )
    .expect("a description");
    let built = t.train_with(&["build", "1"], &env);
    assert!(built.status.success(), "build: {}", text(&built.stderr));
    assert!(
        text(&built.stdout).contains("release 1.2.0, completing calendarlinks"),
        "{}",
        text(&built.stdout)
    );
    assert_eq!(
        t.git(&["log", "-1", "--format=%s", "train"]),
        "release: 1.2.0"
    );
    assert!(
        t.git(&["log", "-1", "--format=%b", "train^"])
            .contains("Completes: calendarlinks"),
        "the merge keeps what its PR completes"
    );
    assert_eq!(
        t.git(&["diff", "--name-only", "train^", "train"]),
        "CHANGELOG.md\napp/release.toml",
        "a release cut writes the changelog and the release file, which no crate compiles \
         (a_release_cut_changes_no_file_a_crate_compiles)"
    );
    let changelog = t.git(&["show", "train:CHANGELOG.md"]);
    assert!(
        changelog.contains("## [Unreleased]\n\n## [1.2.0] - ")
            && changelog.contains("- Completes `todo:suzatary/calendarlinks`: the calendarlinks item\n\n- calendar links"),
        "{changelog}"
    );

    // PR 2 is branched now, before 1.2.0 lands, with an entry under this Unreleased.
    t.pr(2, "a fix", &[("CHANGELOG.md", &changelog_with("a fix"))]);

    // Land checks the chain reads 1.2, pushes master with its tag, then records it done. A
    // local tag of that name, a leftover of a refused land, does not stop it.
    assert!(t.train_with(&["gate"], &env).status.success());
    t.git(&["tag", "v1.2.0", "origin/master"]);
    let landed = t.train_with(&["land"], &env);
    let train = t.git(&["rev-parse", "train"]);
    assert_eq!(t.origin_master(), train, "land: {}", text(&landed.stderr));
    assert_eq!(
        run_git(
            &t.scratch.join("origin.git"),
            &["rev-parse", "v1.2.0^{commit}"]
        ),
        train,
        "the tag is the landed release"
    );
    assert!(
        recorded(&data, "calendarlinks", "PR #1"),
        "done, as the assistant, naming the PR"
    );

    // Recording again records nothing more.
    let again = t.train_with(&["record"], &env);
    assert!(
        again.status.success(),
        "record: {:?} {} {}",
        again.status,
        text(&again.stderr),
        text(&again.stdout)
    );
    assert!(
        text(&again.stdout).contains("nothing more"),
        "{}",
        text(&again.stdout)
    );

    // A train that completes nothing is the next patch. PR 2's entry merges into the section
    // 1.2.0 closed, and the cut moves it back, so it is 1.2.1's.
    let next = t.train_with(&["build", "2"], &env);
    assert!(next.status.success(), "build: {}", text(&next.stderr));
    assert_eq!(
        t.git(&["log", "-1", "--format=%s", "train"]),
        "release: 1.2.1"
    );
    let changelog = t.git(&["show", "train:CHANGELOG.md"]);
    let (newer, older) = changelog.split_at(changelog.find("## [1.2.0]").expect("1.2.0"));
    assert!(
        newer.contains("- a fix") && !older.contains("- a fix"),
        "{changelog}"
    );
}

/// A release completed by hand between build and land is the next train's: the chain reads above
/// the cut, so the land goes ahead, and the tag is the cut.
#[test]
fn train_land_takes_a_chain_that_moved_above_the_cut() {
    let t = Train::new("release-moved");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_releases(&files, "1.1.0");
    t.pr(1, "a fix", &[("a.txt", "a\n")]);
    assert!(t.train_with(&["build", "1"], &env).status.success());
    assert_eq!(
        t.git(&["log", "-1", "--format=%s", "train"]),
        "release: 1.1.1"
    );
    assert!(t.train_with(&["gate"], &env).status.success());
    let closed = crate::binary::keyed()
        .args([
            "--instance",
            &instance.to_string_lossy(),
            "todo",
            "--chain",
            "suzatary",
            "done",
            "calendarlinks",
            "--actor",
            "ada",
        ])
        .output()
        .expect("the binary runs");
    assert!(closed.status.success(), "{}", text(&closed.stderr));
    let landed = t.train_with(&["land"], &env);
    let train = t.git(&["rev-parse", "train"]);
    assert_eq!(t.origin_master(), train, "land: {}", text(&landed.stderr));
    assert_eq!(
        run_git(
            &t.scratch.join("origin.git"),
            &["rev-parse", "v1.1.1^{commit}"]
        ),
        train,
        "the tag is the cut"
    );
}

/// THE BINARY THAT WRITES THE CHAIN IS BUILT FROM A NAMED COMMIT: with no `SUZATARY_TRAIN_BIN`, every
/// app the train asks `scripts/app-bin.sh` for is a commit's, origin/master's at build and the
/// train's at land, and never the checkout as it is.
#[test]
fn train_builds_the_app_from_a_named_commit() {
    let t = Train::new("release-named");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    // The stub app-bin names the commit it was asked for and hands over the real binary.
    let asked = t.scratch.join("asked");
    let app = t.scratch.join("app");
    std::fs::create_dir_all(app.join("bin")).expect("an app");
    std::os::unix::fs::symlink(crate::binary::path(), app.join("bin/suzatary")).expect("a link");
    let stub = format!(
        "#!/bin/sh\necho \"${{SUZATARY_APP_REV:-the checkout}}\" >>'{asked}'\necho '{app}'\n",
        asked = asked.display(),
        app = app.display()
    );
    let mut files = release_files("1.1.0");
    files.push(("scripts/app-bin.sh", stub));
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_releases(&files, "1.1.0");
    t.pr(1, "calendar links", &[("a.txt", "a\n")]);
    std::fs::write(t.scratch.join("prs/1.body"), "Completes: calendarlinks\n").expect("a body");
    let unpinned = |args: &[&str]| {
        let mut cmd = t.command(args, &env);
        cmd.env_remove("SUZATARY_TRAIN_BIN");
        cmd.output().expect("train.sh runs")
    };
    let master = t.origin_master();
    let built = unpinned(&["build", "1"]);
    assert!(built.status.success(), "build: {}", text(&built.stderr));
    let at_build = std::fs::read_to_string(&asked).expect("the app was asked for");
    assert!(
        !at_build.is_empty() && at_build.lines().all(|rev| rev == master),
        "build asks for origin/master's app alone: {at_build}"
    );
    std::fs::remove_file(&asked).expect("the log goes");
    assert!(unpinned(&["gate"]).status.success());
    let train = t.git(&["rev-parse", "train"]);
    let landed = unpinned(&["land"]);
    assert_eq!(t.origin_master(), train, "land: {}", text(&landed.stderr));
    let at_land = std::fs::read_to_string(&asked).expect("the app was asked for");
    assert!(
        at_land.lines().any(|rev| rev == train)
            && at_land.lines().all(|rev| rev == train || rev == master),
        "land checks and records with the train's app: {at_land}"
    );
    assert!(
        recorded(&data, "calendarlinks", "PR #1"),
        "done, as the assistant, naming the PR"
    );
}

/// A cut whose tag origin already holds is refused at build, not after the whole gate at land.
#[test]
fn train_build_refuses_a_cut_origin_already_tagged() {
    let t = Train::new("release-tagged");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_releases(&files, "1.1.0");
    // A tag of the version the chain reads, on a commit no release on master names.
    run_git(&t.author(), &["tag", "v1.1.1", "HEAD"]);
    run_git(&t.author(), &["push", "-q", "origin", "v1.1.1"]);
    t.pr(1, "a fix", &[("a.txt", "a\n")]);
    let built = t.train_with(&["build", "1"], &env);
    assert!(
        !built.status.success(),
        "a cut origin already tagged was built"
    );
    assert!(
        text(&built.stderr).contains("origin has v1.1.1 already"),
        "{}",
        text(&built.stderr)
    );
}

/// `record <rev>` records only what landed: a release commit off origin/master's first-parent
/// chain is refused, and writes nothing.
#[test]
fn train_record_refuses_what_never_landed() {
    let t = Train::new("release-offchain");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_releases(&files, "1.1.0");
    t.git(&["fetch", "-q", "origin"]);
    t.git(&["checkout", "-q", "-B", "aside", "origin/master"]);
    write(
        &t.work(),
        "app/release.toml",
        "version = \"1.2.0\"\ncompletes = [\"calendarlinks\"]\n",
    );
    t.git(&["add", "-A"]);
    t.git(&[
        "commit",
        "-q",
        "-m",
        "release: 1.2.0",
        "-m",
        "Release: 1.2.0",
    ]);
    let aside = t.git(&["rev-parse", "HEAD"]);
    t.git(&["checkout", "-q", "--detach", "origin/master"]);
    let refused = t.train_with(&["record", &aside], &env);
    assert!(
        !refused.status.success(),
        "an unlanded release was recorded"
    );
    assert!(
        text(&refused.stderr).contains("never landed"),
        "{}",
        text(&refused.stderr)
    );
    assert!(
        !completed(&data, "calendarlinks"),
        "nothing is written for it"
    );
}

/// The count reads origin's newest release alone, and a land whose record failed is repaired by
/// the next command: a local tag moves nothing, and the next build records what the land did
/// not.
#[test]
fn train_reads_origin_first_and_repairs_a_failed_record() {
    let t = Train::new("release-sync");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let flag = t.scratch.join("fail-record");
    let failing = suzatary_testkit::fake_program(
        &t.scratch.join("bin"),
        "suzatary-failing",
        &format!(
            "#!/bin/sh\n[ \"$1 $2\" != \"release record\" ] || [ ! -e '{flag}' ] || {{ echo 'the chain is busy' >&2; exit 1; }}\nexec '{real}' \"$@\"\n",
            flag = flag.display(),
            real = crate::binary::path()
        ),
    );
    let mut env = release_env(&instance);
    env.push(("SUZATARY_TRAIN_BIN", failing.display().to_string()));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_moves(&files);
    t.pr(1, "calendar links", &[("a.txt", "a\n")]);
    std::fs::write(t.scratch.join("prs/1.body"), "Completes: calendarlinks\n").expect("a body");
    t.pr(2, "a fix", &[("b.txt", "b\n")]);

    assert!(t.train_with(&["build", "1"], &env).status.success());
    assert!(t.train_with(&["gate"], &env).status.success());
    std::fs::write(&flag, "").expect("the flag");
    let landed = t.train_with(&["land"], &env);
    assert!(
        !landed.status.success(),
        "a land whose record failed says so"
    );
    assert!(
        text(&landed.stderr).contains("scripts/train.sh record"),
        "{}",
        text(&landed.stderr)
    );
    assert_eq!(
        run_git(
            &t.scratch.join("origin.git"),
            &["rev-parse", "v1.2.0^{commit}"]
        ),
        t.origin_master(),
        "master and the tag landed"
    );
    assert!(
        !recorded(&data, "calendarlinks", "PR #1"),
        "the record failed"
    );

    // No local v1.2.0, a stale local v1.7.0, and the record no longer failing: the next build
    // counts from origin's tag and records the land's todo first.
    std::fs::remove_file(&flag).expect("the flag goes");
    t.git(&["tag", "-d", "v1.2.0"]);
    t.git(&["tag", "-f", "v1.7.0", "origin/master"]);
    let next = t.train_with(&["build", "2"], &env);
    assert!(next.status.success(), "build: {}", text(&next.stderr));
    assert!(
        recorded(&data, "calendarlinks", "PR #1"),
        "the next command recorded it"
    );
    assert_eq!(
        t.git(&["log", "-1", "--format=%s", "train"]),
        "release: 1.2.1",
        "counted from origin's v1.2.0, not the local v1.7.0"
    );
}

/// A train whose merges are each green and whose release commit alone is red names the release.
#[test]
fn train_bisect_names_a_red_release() {
    let t = Train::new("release-bisect");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let mut env = release_env(&instance);
    env.push(("GATE_RED_TEXT", "the calendarlinks item".to_owned()));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_moves(&files);
    t.pr(1, "calendar links", &[("a.txt", "a\n")]);
    std::fs::write(t.scratch.join("prs/1.body"), "Completes: calendarlinks\n").expect("a body");
    t.pr(2, "a fix", &[("b.txt", "b\n")]);
    assert!(t.train_with(&["build", "1", "2"], &env).status.success());
    assert!(
        !t.train_with(&["gate"], &env).status.success(),
        "the release is red"
    );
    let bisect = t.train_with(&["bisect"], &env);
    let said = text(&bisect.stdout);
    assert!(
        said.contains("broke it: release: 1.2.0"),
        "{said}{}",
        text(&bisect.stderr)
    );
    assert!(
        said.contains("green in the tree half: master and every merge"),
        "{said}"
    );
}

/// A PR whose `Completes:` line names something that is not a todo id rides no train.
#[test]
fn train_build_skips_a_pr_completing_what_is_no_todo() {
    let t = Train::new("release-bad-id");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    std::fs::write(t.scratch.join("prs/1.body"), "Completes: Calendar Links\n").expect("a body");
    t.pr(2, "the second", &[("b.txt", "b\n")]);
    let out = t.train(&["build", "1", "2"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let row = text(&out.stdout)
        .lines()
        .find(|l| l.starts_with("#1"))
        .map(str::to_owned)
        .unwrap_or_default();
    assert!(row.contains("skipped") && row.contains("Calendar"), "{row}");
    assert_eq!(t.merges(), vec!["Merge #2: the second".to_owned()]);
}

/// A joined merge keeps what its PR completes (git reads trailers from the last paragraph
/// alone), a description written with CRLF reads the same, and a PR naming a todo the chain
/// does not hold is skipped alone.
#[test]
fn train_build_keeps_completes_through_a_join_and_skips_only_an_unknown_todo() {
    let t = Train::new("release-joined");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_moves(&files);

    // #2 is written before master takes an entry of its own, so its merge is joined.
    t.pr(
        2,
        "calendar links",
        &[("CHANGELOG.md", &changelog_with("two"))],
    );
    t.master_moves(&[("CHANGELOG.md", &changelog_with("one"))]);
    std::fs::write(
        t.scratch.join("prs/2.body"),
        "Links.\r\n\r\nCompletes: calendarlinks\r\n",
    )
    .expect("a description");
    t.pr(3, "a wish", &[("c.txt", "c\n")]);
    std::fs::write(t.scratch.join("prs/3.body"), "Completes: nosuchtodo\n").expect("a body");

    let built = t.train_with(&["build", "2", "3"], &env);
    let stdout = text(&built.stdout);
    assert!(
        built.status.success(),
        "build: {}{stdout}",
        text(&built.stderr)
    );
    let row = |n: &str| {
        stdout
            .lines()
            .find(|l| l.starts_with(n))
            .unwrap_or_default()
            .to_owned()
    };
    assert!(row("#2").contains("joined"), "{stdout}");
    assert!(
        row("#3").contains("skipped") && row("#3").contains("nosuchtodo"),
        "{stdout}"
    );
    let joined = t.git(&[
        "log",
        "-1",
        "--format=%(trailers:key=Completes,valueonly)",
        "train^",
    ]);
    assert_eq!(
        joined.trim(),
        "calendarlinks",
        "the joined merge's trailers"
    );
    assert!(
        stdout.contains("release 1.2.0, completing calendarlinks"),
        "{stdout}"
    );
    assert_eq!(
        t.git(&[
            "log",
            "-1",
            "--format=%(trailers:key=Release,valueonly)",
            "train"
        ])
        .trim(),
        "1.2.0"
    );
}

/// Whether the fixture's project chain holds `todo` completed as `assistant` with `note`.
/// Whether anyone recorded `todo` done on the project chain under `data`.
fn completed(data: &Path, todo: &str) -> bool {
    suzatary_core::record::EventStore::new(
        data.join("events-suzatary"),
        prodrome_core::policy::Untrusted::none(),
    )
    .events()
    .expect("the chain reads")
    .iter()
    .any(|e| matches!(e, suzatary_core::record::TodoEvent::Completed(done) if done.todo.as_str() == todo))
}

fn recorded(data: &Path, todo: &str, note: &str) -> bool {
    suzatary_core::record::EventStore::new(
        data.join("events-suzatary"),
        prodrome_core::policy::Untrusted::none(),
    )
    .events()
    .expect("the chain reads")
    .iter()
    .any(|e| {
        matches!(e, suzatary_core::record::TodoEvent::Completed(done)
            if done.todo.as_str() == todo && done.actor.as_str() == "assistant" && done.note == note)
    })
}

impl Train {
    /// The full gates' runs, one `<flag> <commit>` line each.
    fn full_runs(&self) -> Vec<String> {
        std::fs::read_to_string(self.scratch.join("probes.full"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Records that a full gate passed `seconds_ago`.
    fn full_passed(&self, seconds_ago: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_secs();
        write(
            &self.work(),
            ".git/train/full-at",
            &format!("{}\n", now - seconds_ago),
        );
    }
}

/// The first train gated when no tree that passed a full gate has landed in the last day is gated
/// full; landing it records the day, and the trains after it are selective until a day passes.
#[test]
fn train_gate_runs_full_once_a_day() {
    let t = Train::new("full-daily");
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    let first = t.train(&["gate"]);
    assert!(first.status.success(), "{}", text(&first.stderr));
    assert!(
        text(&first.stderr).contains("no tree that passed a full gate has landed in the last day")
    );
    assert_eq!(t.full_runs().len(), 2, "both halves ran full");
    assert!(
        !t.work().join(".git/train/full-at").exists(),
        "a gate records nothing: only a landed tree does"
    );
    let landed = t.train(&["land"]);
    assert!(landed.status.success(), "{}", text(&landed.stderr));

    t.pr(2, "the second", &[("b.txt", "b\n")]);
    assert!(t.train(&["build", "2"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert_eq!(
        t.full_runs().len(),
        2,
        "a fully passed tree landed today: this gate is selective"
    );

    t.full_passed(86_400);
    t.pr(3, "the third", &[("c.txt", "c\n")]);
    assert!(t.train(&["build", "2", "3"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert_eq!(
        t.full_runs().len(),
        4,
        "a day later the gate runs full again"
    );
}

/// A full gate that fails where a selective one passed the same tree is a finding, and nothing
/// lands over it; a red full gate is not a full pass, so the next gate runs full again.
#[test]
fn train_full_red_beside_a_selective_green_is_a_finding() {
    let t = Train::new("full-finding");
    t.full_passed(0);
    t.pr(1, "the first", &[("missed", "x\n")]);
    let seed = t.origin_master();
    assert!(t.train(&["build", "1"]).status.success());
    assert!(
        t.train(&["gate"]).status.success(),
        "the selection passes it"
    );
    assert!(t.full_runs().is_empty());

    let full = t.train(&["gate", "--full"]);
    assert!(!full.status.success());
    assert!(
        text(&full.stderr).contains("not a function of the tree"),
        "{}",
        text(&full.stderr)
    );
    let refused = t.train(&["land"]);
    assert!(!refused.status.success(), "a finding lands nothing");
    assert_eq!(t.origin_master(), seed);
    assert!(!t.train(&["gate"]).status.success());
    assert_eq!(
        t.full_runs().len(),
        4,
        "the gate after a red full gate runs full"
    );
}

/// THE TRAIN'S APP IS THE ONE THE GATE BUILT: `scripts/app-bin.sh` builds a named commit
/// (`SUZATARY_APP_REV`) from that commit's public tree (`scripts/public-tree.sh --store`), the
/// store path the merge gate's deploy-build builds and roots, never from `git+file` of the
/// private repository, a second build of the same binary.
#[test]
fn app_bin_builds_a_named_commit_from_its_public_tree() {
    let scratch = Scratch::new("app-bin-public");
    let checkout = scratch.join("checkout");
    std::fs::create_dir_all(checkout.join("scripts")).expect("a checkout");
    suzatary_testkit::copy_program(
        &repo().join("scripts/app-bin.sh"),
        &checkout.join("scripts/app-bin.sh"),
    );
    let tree = scratch.join("public-tree");
    let asked = scratch.join("asked");
    let _ = suzatary_testkit::fake_program(
        &checkout.join("scripts"),
        "public-tree.sh",
        &format!(
            "#!/bin/sh\necho \"public $*\" >>'{asked}'\necho '{tree}'\n",
            asked = asked.display(),
            tree = tree.display()
        ),
    );
    let _ = suzatary_testkit::fake_program(
        &checkout.join("bin"),
        "nix",
        &format!(
            "#!/bin/sh\necho \"nix $*\" >>'{asked}'\necho /nix/store/example-suzatary\n",
            asked = asked.display()
        ),
    );
    let path = format!(
        "{}:{}",
        checkout.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let ran = Command::new(checkout.join("scripts/app-bin.sh"))
        .env("PATH", path)
        .env("SUZATARY_REPO", &checkout)
        .env("SUZATARY_ROOTS", scratch.join("roots"))
        .env("SUZATARY_APP_REV", "0123456789abcdef")
        .output()
        .expect("app-bin.sh runs");
    assert!(ran.status.success(), "{}", text(&ran.stderr));
    assert_eq!(text(&ran.stdout).trim(), "/nix/store/example-suzatary");
    let asked = std::fs::read_to_string(&asked).expect("the stubs were asked");
    let lines: Vec<&str> = asked.lines().collect();
    assert_eq!(lines[0], "public --store 0123456789abcdef", "{asked}");
    assert!(
        lines[1].starts_with(&format!("nix build {}#suzatary ", tree.display())),
        "the public tree's package is built: {asked}"
    );
}
