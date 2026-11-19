//! The merge queue (`scripts/train.sh`), run as the real script against a throwaway
//! origin: a bare repository whose `refs/pull/<n>/head` are the pull requests, a stub
//! `gh` that answers their titles and plays GitHub, and a stub gate whose halves' verdicts
//! are functions of the tree.

use super::{Scratch, repo};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// An origin, an author's clone that writes the pull requests and moves master, and the
/// operator's clone the train is built in.
struct Train {
    scratch: Scratch,
}

/// The stub `gh`: `pr view` answers from `prs/<n>` and `prs/<n>.body`. `api`, `workflow run` and
/// `run list` play GitHub's Actions over the gate branches pushed to `core.git`, logging each call
/// to `github.log`. A gate run runs the stub gate's tree half once in `runner-<sha>`, a checkout of
/// that commit alone, as gate.yml's jobs do. `GH_RUN` and `GH_CACHE` choose how the gate run and the
/// publication run end; each case below is one way a real run has ended.
const GH: &str = r#"#!/bin/sh
exec 9>&-
here="$(cd "$(dirname "$0")/.." && pwd)"
if [ "$1 $2" = "workflow run" ]; then
  [ "$3 $7" = "publication.yml master" ] || exit 64
  echo "dispatch $7 ${9#commit=}" >>"$TRAIN_PROBES.args"
  [ "$GH_CACHE" != down ] || { echo "error connecting to api.github.com" >&2; exit 1; }
  mkdir -p "$here/dispatched" && echo "${GH_CACHE:-success}" >"$here/dispatched/${9#commit=}"
  [ "$GH_CACHE" != unanswered ] || exit 124
  exit 0
fi
if [ "$1 $2" = "run list" ]; then
  [ "$6 $8" = "publication.yml workflow_dispatch" ] || exit 64
  sha="$(echo "$*" | sed -n 's/.*Publication cache \([0-9a-f]\{40\}\).*/\1/p')"
  echo "publication $sha" >>"$TRAIN_PROBES.args"
  [ "$GH_CACHE" != down ] || { echo "error connecting to api.github.com" >&2; exit 1; }
  [ -e "$here/dispatched/$sha" ] || exit 0
  case "$(cat "$here/dispatched/$sha")" in
    failure) echo "https://github.test/runs/pub-$sha completed failure" ;;
    running) echo "https://github.test/runs/pub-$sha in_progress " ;;
    *) echo "https://github.test/runs/pub-$sha completed success" ;;
  esac
  exit 0
fi
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
          checks-red) echo "in_progress " >"$here/runs/$sha" && : >"$here/runs/$sha.checks" ;;
          e2e-red) echo "in_progress " >"$here/runs/$sha" && : >"$here/runs/$sha.e2e" ;;
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
    repos/*/actions/runs\?per_page=*)
      for f in "$here"/runs/*; do
        [ -f "$f" ] && [ "$(cat "$f")" = "in_progress " ] && basename "$f"
      done
      for f in "$here"/dispatched/*; do
        [ -f "$f" ] && [ "$(cat "$f")" = running ] && echo "pub-$(basename "$f")"
      done
      exit 0 ;;
    repos/*/actions/runs/*/cancel) ;;
    repos/*/actions/runs/*/jobs*)
      run="${2#*/runs/}" && run="${run%%/*}"
      [ ! -e "$here/runs/$run.red" ] || echo "7 gate (tests)"
      [ ! -e "$here/runs/$run.checks" ] || echo "8 gate (checks)"
      [ ! -e "$here/runs/$run.e2e" ] || echo "9 gate (e2e-1)" ;;
    repos/*/actions/runs/*) cat "$here/runs/${2##*/}" ;;
    repos/*/actions/jobs/7/logs) sed 's/^/2026-10-20T10:00:00.1234567Z /' "$(ls -t "$here"/runs/*.log | head -n 1)" ;;
    repos/*/actions/jobs/9/logs)
      printf '%b\n' '##[group]Run nix develop .#triage -c scripts/merge-gate.sh --no-live --steps "$STEPS"' \
        '==> e2e (quiet lane)' '\033[32m✔ list: a row (12ms)\033[39m' '\033[31m✖ destinations: the inbox opens (240100ms)\033[39m' \
        '  Error: waited 240000ms for the inbox' '    last: it stayed false' '      at TestContext.<anonymous> (file:///b.mjs:1:1)' \
        '##[error]Process completed with exit code 1.' | sed 's/^/2026-10-20T10:00:00.1234567Z /' ;;
    repos/*/actions/jobs/8/logs)
      printf '%s\n' '##[group]Run nix develop .#triage -c scripts/merge-gate.sh --no-live --steps "$STEPS"' \
        '==> clippy: FAILED (101)   42s' 'error[E0308]: mismatched types' \
        "==> the failing steps' last lines" '--- clippy' 'error[E0308]: mismatched types' \
        'size: rust 1 characters, 1 lines' 'free after the gate: 9G' '##[group]Run exit 1' \
        '##[error]Process completed with exit code 1.' | sed 's/^/2026-10-20T10:00:00.1234567Z /' ;;
    *) exit 64 ;;
  esac
  exit 0
fi
case "$1 $2" in
  "pr comment") mkdir -p "$here/comments" && printf '%s\n' "$5" >>"$here/comments/$3" ;;
  "pr view")
    if [ "$5" = body ]; then cat "$here/prs/$3.body" 2>/dev/null; exit 0; fi
    exec cat "$here/prs/$3" ;;
  *) exit 64 ;;
esac
"#;

/// The stub merge gate. A half is red where the tree holds the file its case below names, and a
/// red run names its one step, `stub`, as the real gate's summary does. Each run records its
/// environment, arguments and directory beside `<TRAIN_PROBES>`. `GATE_HOLD` and `GATE_HOLD_LIVE`
/// hold a half on a fifo until the test lets it go on.
const GATE: &str = r#"#!/bin/sh
env >"$TRAIN_PROBES.env"
echo "$*" >>"$TRAIN_PROBES.args"
full=""
if [ "$1" = --full ] || [ "$1" = --nightly ]; then full="$1"; shift; echo "$1 $(git rev-parse HEAD) $full" >>"$TRAIN_PROBES.full"; fi
echo "$1 $(git rev-parse HEAD) $(pwd)" >>"$TRAIN_PROBES"
[ -z "$full" ] || [ ! -e missed ] || { echo "missed is in the tree" >&2; exit 1; }
[ -z "$GATE_HOLD" ] || [ "$1" != --no-live ] || { echo up >"$GATE_HOLD.up"; cat "$GATE_HOLD" >/dev/null; }
case "$1" in
  --no-live)
    bad=broken
    case "$2 $3" in
      "--steps "*changelog*) bad=broken-box ;;
      "--steps "*public-tree*) bad=broken-build ;;
    esac ;;
  --only-live)
    bad=broken-live
    case "$2 $3" in "--steps live,"*) bad=broken-live-early ;; esac ;;
  *) exit 64 ;;
esac
[ -z "$GATE_HOLD_LIVE" ] || [ "$bad" != broken-live ] || { echo $$ >"$GATE_HOLD_LIVE.pid"; echo up >"$GATE_HOLD_LIVE.up"; cat "$GATE_HOLD_LIVE" >/dev/null; }
[ -z "$GATE_RED_TEXT" ] || [ "$1" != --no-live ] || ! grep -qs "$GATE_RED_TEXT" CHANGELOG.md || bad=CHANGELOG.md
test -e "$bad" || exit 0
[ "$bad" != broken-live ] || {
  mkdir -p "$SUZATARY_GATE_EVIDENCE/stub-$(git rev-parse HEAD | cut -c1-12)"
  printf '\033[31m✖ pages: a *starred* #scene (12ms)\033[39m\n  Error: the page said something\n' >&2
}
printf 'stub             FAILED (1)\n==> the failing steps'"'"' last lines\n--- stub\n%s is in the tree\n' "$bad" >&2
exit 1
"#;

/// The stub public-tree script: `<out> <rev>` writes `<rev>`'s tree without its `private/`,
/// and `--check --strict <dir>` exits 1, naming the file, where a file of it says `leak`. With
/// `PUBLIC_UNWRITTEN` set no tree is written, and with `PUBLIC_UNCHECKED` the check exits 2,
/// the real one's "could not run whole".
const PUBLIC: &str = r#"#!/bin/sh
if [ "$1" = --check ]; then
  echo "${SUZATARY_BIN:-}" >>"$TRAIN_PROBES.matcher"
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
/// A lockfile cargo reads for a one-package workspace with no dependency.
const LOCK: &str = "version = 4\n\n[[package]]\nname = \"fixture\"\nversion = \"0.0.0\"\n";

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
        // A release's files, which a land publishes before it records the release.
        let app = tree(&format!(
            "100644 blob {blob}\tCargo.toml\n100644 blob {blob}\trelease.toml\n"
        ));
        let scripts = tree(&format!("100755 blob {blob}\tapp-bin.sh\n"));
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
        .chain([
            format!("040000 tree {app}\tapp\n"),
            format!("040000 tree {docs}\tdocs\n"),
            format!("040000 tree {scripts}\tscripts\n"),
        ])
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

/// Whether the process <pid> still runs, a zombie counted as gone.
fn runs(pid: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.rsplit_once(") ")
            .is_some_and(|(_, rest)| !rest.starts_with('Z'))
    })
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

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

#[test]
fn train_build_merges_by_head_and_skips_a_conflict() {
    let t = Train::new("build");
    // Each conflicts with master, so it is skipped whatever order the PRs merge in.
    t.pr(3, "the third", &[("README.md", "three\n")]);
    // A released entry rewritten, as master rewrote it too.
    let rewritten = CHANGELOG.replace("the first release", "a rewrite");
    t.pr(4, "the fourth", &[("CHANGELOG.md", &rewritten)]);
    let moved = CHANGELOG.replace("the first release", "master's rewrite");
    t.master_moves(&[("README.md", "master's\n"), ("CHANGELOG.md", &moved)]);
    let long = "a title long enough that the merge subject must be cut to fit seventy-two";
    t.pr(
        1,
        "the first",
        &[
            ("changelog.d/one.md", "### Added\n- one\n"),
            ("a.txt", "a\n"),
        ],
    );
    t.pr(
        2,
        long,
        &[
            ("changelog.d/two.md", "### Added\n- two\n"),
            ("b.txt", "b\n"),
        ],
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
    // Two entries are two files, so their merges cannot conflict.
    assert_eq!(
        t.git(&["ls-tree", "--name-only", "train", "changelog.d/"]),
        "changelog.d/one.md\nchangelog.d/two.md",
        "both fragments merged"
    );

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

/// A PR's `Wasm-growth:` lines ride on its merge commit as trailers, each as written, where the
/// wasm budget's guard reads them; a PR declaring none carries none.
#[test]
fn train_build_carries_a_pr_s_declared_wasm_growth_on_its_merge() {
    let t = Train::new("wasm-growth");
    t.pr(3, "grows the pages", &[("a.txt", "a\n")]);
    std::fs::write(
        t.scratch.join("prs/3.body"),
        "The topic bar.\n\nWasm-growth: views_bg.wasm +19000: the topic bar's sheet\nWasm-growth: prodrome_bg.wasm +5000: held values\n",
    )
    .expect("a body");
    t.pr(4, "beside", &[("c.txt", "c\n")]);
    let out = t.train(&["build", "3", "4"]);
    assert!(out.status.success(), "build: {}", text(&out.stderr));
    let merge = |n: u32| {
        t.git(&[
            "log",
            "--first-parent",
            "--format=%B%x00",
            "origin/master..train",
        ])
        .split('\0')
        .find(|m| m.contains(&format!("Merge #{n}:")))
        .unwrap_or_else(|| panic!("no merge of #{n}"))
        .to_owned()
    };
    let grown = merge(3);
    assert!(
        grown.contains("Wasm-growth: views_bg.wasm +19000: the topic bar's sheet")
            && grown.contains("Wasm-growth: prodrome_bg.wasm +5000: held values"),
        "both lines, as written: {grown}"
    );
    assert!(
        !merge(4).contains("Wasm-growth"),
        "none declared: {}",
        merge(4)
    );
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
        within_two_minutes(|| Command::new("flock")
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
    assert_eq!(
        t.probes("--only-live"),
        0,
        "a red tree half keeps the live half from starting"
    );
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
    assert_eq!(t.probes("--only-live"), 0, "the live half is not probed");

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

/// The output of `child`, a train run in `t`, which must end within two minutes; one that does not
/// is killed and fails the test, so a half that was never stopped fails it rather than hangs it.
/// The failure says what was still alive and, where the train writes to `train.stderr` in the
/// scratch, its last lines.
fn ends_within_two_minutes(t: &Train, child: std::process::Child) -> Output {
    let pid = child.id();
    let (said, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || said.send(child.wait_with_output()));
    let Ok(out) = ended.recv_timeout(std::time::Duration::from_secs(120)) else {
        let alive = alive(pid, &t.scratch);
        let printed = std::fs::read_to_string(t.scratch.join("train.stderr")).unwrap_or_default();
        let last = printed.lines().rev().take(20).collect::<Vec<_>>();
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
        panic!(
            "the train did not end within two minutes: a half it should have stopped still runs\n\
             still alive:\n{alive}\nits last lines:\n{}",
            last.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    };
    out.expect("the train ends")
}

/// Each process of `train`'s tree or holding a file under `scratch` (an orphan of the train's
/// included), a line each: pid, parent, group, state, kernel wait channel, the syscall it is in
/// (61 is wait4, 0 read), its command line, and its descriptors that are not /dev/null.
fn alive(train: u32, scratch: &Path) -> String {
    let scratch = scratch
        .canonicalize()
        .unwrap_or_else(|_| scratch.to_owned());
    let read = |pid: &str, file: &str| {
        std::fs::read_to_string(format!("/proc/{pid}/{file}")).unwrap_or_default()
    };
    let procs = std::fs::read_dir("/proc")
        .expect("/proc reads")
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_string_lossy().into_owned();
            pid.parse::<u32>().ok()?;
            let stat = read(&pid, "stat");
            let (_, rest) = stat.rsplit_once(") ")?;
            let fields = rest
                .split(' ')
                .take(3)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            (fields.len() == 3).then_some((pid, fields))
        })
        .collect::<Vec<_>>();
    let mut tree = BTreeSet::from([train.to_string()]);
    while let Some(child) = procs
        .iter()
        .find(|(pid, f)| tree.contains(&f[1]) && !tree.contains(pid))
    {
        tree.insert(child.0.clone());
    }
    procs
        .iter()
        .filter_map(|(pid, f)| {
            let fds = fds(pid);
            let under = |to: &Path| to.starts_with(&scratch);
            let ours = tree.contains(pid)
                || fds.iter().any(|(_, to)| under(to))
                || std::fs::read_link(format!("/proc/{pid}/cwd")).is_ok_and(|cwd| under(&cwd));
            ours.then(|| {
                let open = fds
                    .iter()
                    .filter(|(_, to)| to != Path::new("/dev/null"))
                    .map(|(fd, to)| format!("{fd}:{}", to.display()))
                    .collect::<Vec<_>>();
                format!(
                    "{pid} ppid {} pgid {} {} {} syscall {} {} [{}]",
                    f[1],
                    f[2],
                    f[0],
                    read(pid, "wchan"),
                    read(pid, "syscall")
                        .split(' ')
                        .next()
                        .unwrap_or_default()
                        .trim(),
                    read(pid, "cmdline").replace('\0', " ").trim(),
                    open.join(" ")
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The open descriptors of the process `pid`, each with what it names.
fn fds(pid: &str) -> Vec<(String, PathBuf)> {
    std::fs::read_dir(format!("/proc/{pid}/fd"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|fd| {
            let to = std::fs::read_link(fd.path()).ok()?;
            Some((fd.file_name().to_string_lossy().into_owned(), to))
        })
        .collect()
}

/// The processes that hold `path` open, by pid.
fn holders(path: &Path) -> Vec<String> {
    let path = path.canonicalize().expect("the path");
    std::fs::read_dir("/proc")
        .expect("/proc reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|pid| fds(pid).iter().any(|(_, to)| *to == path))
        .collect()
}

/// Asserts the live half held on `hold` was stopped whole: the stub gate's process is gone, and
/// so is every process that held its fifo (its `cat`, a child the gate's pid alone would miss).
fn live_half_stopped(hold: &Path) {
    let pid = std::fs::read_to_string(hold.with_extension("pid")).expect("the live half ran");
    assert!(!runs(pid.trim()), "the live half's gate {pid} was stopped");
    assert_eq!(
        holders(hold),
        Vec::<String>::new(),
        "no process of the live half is left"
    );
}

/// Polls `done` for two minutes.
fn within_two_minutes(done: impl Fn() -> bool) -> bool {
    (0..1200).any(|_| {
        done() || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            false
        }
    })
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

/// `gate --remote github`: the box runs its steps of the tree half that build nothing first, then
/// the train commit's public tree goes to core, committed on core's master by the publications'
/// author with a `Gate:` trailer, pushed to `gate/<train sha>` and to no other ref. The box waits
/// for the publication run of that commit, and only then runs the steps that build the public
/// tree, and then the live half, whose deploy-build builds it too. The runner gates that commit
/// alone, its success is the half's green, and the branch is deleted once the run is read. The
/// train lands.
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
        text(&out.stderr).contains("succeeded: the box's builds come from the binary cache"),
        "{}",
        text(&out.stderr)
    );
    let refs = t.core_pushes();
    let branch = format!("refs/heads/gate/{train}");
    assert_eq!(refs.len(), 2, "one push and one deletion: {refs:?}");
    let pushed = refs[0]
        .strip_prefix(&format!("{branch} "))
        .unwrap_or_else(|| panic!("only gate/<train sha> is pushed: {refs:?}"))
        .to_owned();
    assert_eq!(refs[1], format!("{branch} {ZERO}"), "and then deleted");
    let order: Vec<String> = t
        .gate_args()
        .into_iter()
        .map(|args| args.trim_start_matches("--full ").to_owned())
        .filter(|args| args != "--no-live --steps cloud")
        .collect();
    assert_eq!(
        order,
        [
            "--no-live --steps fmt,changelog".to_owned(),
            format!("dispatch master {pushed}"),
            "--only-live --steps live,live-all,live-guards".to_owned(),
            format!("publication {pushed}"),
            "--no-live --steps public-tree,deployed-shape".to_owned(),
            "--only-live --steps web-live,deploy-build,push-dry,public-check".to_owned(),
        ],
        "the box dispatches master's publication run, runs the live steps that read none of its \
         builds while it builds, and builds the public tree once it ended"
    );
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
    assert!(stderr.contains("tree half: failed"), "{stderr}");
    assert!(
        stderr.contains("broken is in the tree") && stderr.contains("red: gate (tests)"),
        "the job's failing lines: {stderr}"
    );
    assert_eq!(t.core(&["for-each-ref", "refs/heads/gate/"]), "");
    let refused = t.train(&["land"]);
    assert!(text(&refused.stderr).contains("tree: failed"));
    assert_eq!(t.origin_master(), seed, "nothing landed");
}

/// The `checks` job red while the run's other jobs still run (a compile break): the tree half is
/// red at once, the live half here is stopped whole, the run's other jobs are cancelled, and the
/// report prints the job's own failing lines, the gate's block and not what GitHub ran after it.
#[test]
fn train_gate_github_red_checks_job_stops_the_live_half_at_once() {
    let t = Train::new("github-checks-red");
    t.on_github();
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    let seed = t.origin_master();
    assert!(t.train(&["build", "1"]).status.success());

    // Polled once a second, so the live half is up before the job's red is read.
    let (hold, _said) = holding(&t, "hold-live");
    let hold_path = hold.to_str().expect("a UTF-8 path");
    let gating = t
        .command(
            &["gate", "--remote", "github"],
            &[
                ("GH_RUN", "checks-red"),
                ("GATE_HOLD_LIVE", hold_path),
                ("SUZATARY_TRAIN_POLL", "1"),
            ],
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("train.sh starts");
    let red = ends_within_two_minutes(&t, gating);
    let stderr = text(&red.stderr);
    assert!(!red.status.success(), "{stderr}");
    assert!(
        stderr.contains("red: gate (checks)")
            && stderr.contains("the tree half is red, so the live half is stopped")
            && stderr.contains("tree half: failed")
            && stderr.contains("live half: unknown"),
        "{stderr}"
    );
    let report = &stderr[stderr.find("tree half: failed").expect("the report")..];
    assert!(
        report.contains("--- job gate (checks)")
            && report.contains("==> the failing steps' last lines\n--- clippy\nerror[E0308]")
            && !report.contains("free after the gate")
            && !report.contains("##[error]"),
        "the report prints the job's failing lines alone: {report}"
    );
    live_half_stopped(&hold);
    let pushed = t
        .core_pushes()
        .first()
        .and_then(|line| line.split(' ').nth(1))
        .expect("the gate commit")
        .to_owned();
    assert!(
        t.github_calls()
            .iter()
            .any(|call| call.ends_with(&format!("/actions/runs/{pushed}/cancel"))),
        "the run's other jobs were cancelled: {:?}",
        t.github_calls()
    );
    assert_eq!(t.core(&["for-each-ref", "refs/heads/gate/"]), "");
    assert!(text(&t.train(&["land"]).stderr).contains("tree: failed"));
    assert_eq!(t.origin_master(), seed, "nothing landed");
}

/// The live half red while the run on GitHub's runners has not ended: the run is cancelled at once
/// and the train is red, rather than waiting on a run whose verdict could not land the tree.
#[test]
fn train_gate_github_red_live_half_stops_the_run() {
    let t = Train::new("github-live-red");
    t.on_github();
    t.pr(1, "the first", &[("broken-live", "x\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    // The live half is held until the train waits on the run, then goes on to its red.
    let (hold, _said) = holding(&t, "hold-live");
    let hold_path = hold.to_str().expect("a UTF-8 path");
    let gating = t
        .command(
            &["gate", "--remote", "github"],
            &[("GH_RUN", "running"), ("GATE_HOLD_LIVE", hold_path)],
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("train.sh starts");
    assert!(
        within_two_minutes(|| t
            .github_calls()
            .iter()
            .any(|call| call.contains("/actions/runs/") && !call.contains("head_sha"))),
        "the train waits on the run: {:?}",
        t.github_calls()
    );
    std::fs::write(&hold, "").expect("the held live half goes on");
    let red = ends_within_two_minutes(&t, gating);
    let stderr = text(&red.stderr);
    assert!(!red.status.success(), "{stderr}");
    assert!(
        stderr.contains("the live half is red, so the tree half is stopped")
            && stderr.contains("tree half: unknown")
            && stderr.contains("live half: failed")
            && stderr.contains("broken-live is in the tree"),
        "{stderr}"
    );
    let pushed = t
        .core_pushes()
        .first()
        .and_then(|line| line.split(' ').nth(1))
        .expect("the gate commit")
        .to_owned();
    assert!(
        t.github_calls()
            .iter()
            .any(|call| call.ends_with(&format!("/actions/runs/{pushed}/cancel"))),
        "the run was cancelled: {:?}",
        t.github_calls()
    );
    assert_eq!(t.core(&["for-each-ref", "refs/heads/gate/"]), "");
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

/// The box's steps red: the half is red, nothing goes to core, and the live half never starts.
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
    assert_eq!(t.probes("--only-live"), 0, "and the live half never starts");
}

/// A publication run that failed, ran past the wait, or that GitHub would not start leaves the
/// public tree's builds to the box, which says which, with the run's address and what the build
/// costs, and builds it after the wait, never beside it; the gate is green all the same, since
/// the builds are the same either way. A run still going when the box stops waiting is cancelled.
#[test]
fn train_gate_github_builds_on_the_box_when_the_publication_run_gave_nothing() {
    let cost = "the box builds the public tree itself, about 45 minutes of release build";
    for (how, said) in [
        (
            "failure",
            ["ended failure (https://github.test/runs/pub-", cost],
        ),
        (
            "running",
            [
                "was still in_progress after 0s (https://github.test/runs/pub-",
                cost,
            ],
        ),
        ("down", ["could not start the publication run", cost]),
    ] {
        let t = Train::new(&format!("github-cache-{how}"));
        t.on_github();
        t.pr(1, "the first", &[("a.txt", "a\n")]);
        assert!(t.train(&["build", "1"]).status.success());

        let out = t.train_with(
            &["gate", "--remote", "github"],
            &[("GH_CACHE", how), ("SUZATARY_TRAIN_CACHE_TIMEOUT", "0")],
        );
        let stderr = text(&out.stderr);
        assert!(out.status.success(), "{how}: {stderr}");
        for said in said {
            assert!(stderr.contains(said), "{how}: {said}: {stderr}");
        }
        let args = t.gate_args();
        let asked = args.iter().position(|a| a.starts_with("dispatch master "));
        let built = args
            .iter()
            .position(|a| a.ends_with("--no-live --steps public-tree,deployed-shape"));
        assert!(
            asked.is_some() && asked < built,
            "{how}: the box builds once the wait ended: {args:?}"
        );
        let cancelled = t
            .github_calls()
            .iter()
            .any(|call| call.contains("/runs/pub-") && call.ends_with("/cancel"));
        assert_eq!(
            cancelled,
            how == "running",
            "{how}: only a publication run still going is cancelled: {:?}",
            t.github_calls()
        );
    }
}

/// A dispatch that started the run but whose answer never came (gh killed by its 60 s bound) is
/// no failure to start: the train looks for the run and takes its builds from the cache.
#[test]
fn train_gate_github_looks_for_the_run_of_an_unanswered_dispatch() {
    let t = Train::new("github-cache-unanswered");
    t.on_github();
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());

    let out = t.train_with(
        &["gate", "--remote", "github"],
        &[("GH_CACHE", "unanswered")],
    );
    let stderr = text(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("looking for the run all the same"),
        "{stderr}"
    );
    assert!(
        stderr.contains("succeeded: the box's builds come from the binary cache"),
        "{stderr}"
    );
}

/// The box's build of the public tree red, after the push: the half is red, no run of the gate is
/// waited on, the live half never starts, the branch is deleted, and nothing lands.
#[test]
fn train_gate_github_red_build_on_the_box_is_the_halfs_red() {
    let t = Train::new("github-build-red");
    t.on_github();
    t.pr(1, "the first", &[("broken-build", "x\n")]);
    let seed = t.origin_master();
    assert!(t.train(&["build", "1"]).status.success());

    let out = t.train(&["gate", "--remote", "github"]);
    let stderr = text(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("tree half: failed"), "{stderr}");
    assert!(
        !t.github_calls()
            .iter()
            .any(|call| call.contains("head_sha")),
        "no gate run is waited on: {:?}",
        t.github_calls()
    );
    assert_eq!(
        t.gate_args()
            .iter()
            .filter(|a| a.starts_with("--only-live"))
            .cloned()
            .collect::<Vec<_>>(),
        ["--only-live --steps live,live-all,live-guards"],
        "the live half's early steps ran beside the publication run, and its others never start"
    );
    assert_eq!(t.core(&["for-each-ref", "refs/heads/gate/"]), "");
    assert!(text(&t.train(&["land"]).stderr).contains("tree: failed"));
    assert_eq!(t.origin_master(), seed, "nothing landed");
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
/// Stopped by TERM or HUP, it also stops the wait and cancels its run, even when it was started
/// with both signals ignored, which bash can neither trap nor reset.
#[test]
fn train_gate_github_killed_deletes_its_branch() {
    for signal in ["TERM", "HUP"] {
        let t = Train::new(&format!("github-killed-{signal}"));
        t.on_github();
        t.pr(1, "the first", &[("a.txt", "a\n")]);
        assert!(t.train(&["build", "1"]).status.success());
        let train = t.git(&["rev-parse", "train"]);

        let started = t.command(&["gate", "--remote", "github"], &[("GH_RUN", "running")]);
        let mut ignoring = isolated(Command::new("bash"));
        ignoring
            .args(["-c", "trap '' HUP TERM; exec \"$@\"", "bash"])
            .arg(started.get_program())
            .args(started.get_args())
            .current_dir(t.work())
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create(t.scratch.join("train.stderr")).expect("a log"));
        for (key, value) in started.get_envs() {
            match value {
                Some(value) => ignoring.env(key, value),
                None => ignoring.env_remove(key),
            };
        }
        let gating = ignoring.spawn().expect("train.sh starts");
        let branch = format!("refs/heads/gate/{train}");
        assert!(
            within_two_minutes(|| {
                t.core(&["for-each-ref", "--format=%(refname)", "refs/heads/gate/"]) == branch
                    && t.github_calls()
                        .iter()
                        .any(|call| call.contains("/actions/runs/") && !call.contains("head_sha"))
            }),
            "{signal}: the train pushed its branch and waits on the run: {:?}",
            t.github_calls()
        );
        run(
            Command::new("kill")
                .args(["-s", signal])
                .arg(gating.id().to_string()),
            "stop the train",
        );
        let out = ends_within_two_minutes(&t, gating);
        assert!(
            !out.status.success(),
            "{signal}: a stopped train is not green"
        );
        assert_eq!(
            t.core(&["for-each-ref", "refs/heads/gate/"]),
            "",
            "{signal}: its gate branch went with it"
        );
        let pushed = t
            .core_pushes()
            .first()
            .and_then(|line| line.split(' ').nth(1))
            .expect("the gate commit")
            .to_owned();
        assert!(
            t.github_calls()
                .iter()
                .any(|call| call.ends_with(&format!("/actions/runs/{pushed}/cancel"))),
            "{signal}: and its gate run was cancelled: {:?}",
            t.github_calls()
        );
    }
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

/// A publish admits every held path however many there are: the held list is longer than a
/// pipe's buffer (64 KiB), and an admit of its first path is still found.
#[test]
fn train_publish_admits_more_held_paths_than_a_pipe_holds() {
    let t = Train::new("publish-admit-many");
    t.pr(1, "the start", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(t.train(&["land"]).status.success());

    let paths: Vec<String> = (0..900)
        .map(|n| format!("store/objects/{n:0>64}.py"))
        .collect();
    let files: Vec<(&str, &str)> = paths.iter().map(|path| (path.as_str(), "x\n")).collect();
    t.pr(2, "a store of new objects", &files);
    assert!(t.train(&["build", "2"]).status.success());
    assert!(t.train(&["gate"]).status.success());
    assert!(!t.train(&["land"]).status.success(), "held");

    let mut args = vec!["publish"];
    for path in &paths {
        args.extend(["--admit", path.as_str()]);
    }
    let admitted = t.at_a_terminal(&args);
    assert!(admitted.status.success(), "{}", text(&admitted.stdout));
    assert_eq!(t.core(&["show", &format!("master:{}", paths[0])]), "x");
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

/// The release fixture's instance file: `{data}` and `{key}` are filled in. The roadmap is mounted
/// at its clone's absolute path, as a deployment mounts a public store, and its public names are
/// invented: `ada` writes there as `adalogin`, and `bea` has no public name. Its upkeep chain is a
/// third chain, `ops`, so a filer that spelled the roadmap's name would miss it.
const RELEASE_INSTANCE: &str = r#"repo = "{data}"
[signatures]
root = "{key}"
[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["assistant", "ada", "bea", "triage"]
owner = "ada"
[[chains]]
name = "roadmap"
events = "{data}/roadmap"
untrusted = []
actors = ["adalogin", "assistant"]
owner = "adalogin"
[chains.public_names]
ada = "adalogin"
assistant = "assistant"
[[chains]]
name = "ops"
events = "events-ops"
untrusted = ["triage"]
actors = ["assistant", "ada", "bea", "triage"]
owner = "ada"
[upkeep]
chain = "ops"
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
"#;

/// The plan beside the fixture's roadmap: `alpha` ships 1.1, `calendarlinks` 1.2 and `suzatarytwo`
/// 2.0, all from 1.
const PLAN: &str = "base = 1\n\n[[release]]\ntodo = \"alpha\"\nmajor = 1\nminor = 1\n\n\
                    [[release]]\ntodo = \"calendarlinks\"\nmajor = 1\nminor = 2\n\n\
                    [[release]]\ntodo = \"suzatarytwo\"\nmajor = 2\nminor = 0\n";

/// The deployment a release reads, under `data`: the roadmap, a public store in a clone of its
/// own origin (`roadmap.git`), holding `suzatarytwo` composing three items, one done, with the plan
/// beside it and its pictures drawn; the upkeep chain `ops` holding the private todo `opsonly`; the
/// box-local lists door 1 and door 2 read; and the instance naming them all, as
/// `SUZATARY_INSTANCE` names the box's.
fn a_release_deployment(data: &Path) -> PathBuf {
    use prodrome_core::fpl;
    use suzatary_record::record::{EventStore, GENESIS, MarkupSource, mk_authored};
    let at = prodrome_core::literal::Datetime::new(2026, 9, 1, 9, 0, 0, 0).expect("an instant");
    let todo = |id: &str, actor: &str, spec| {
        mk_authored(
            id,
            at,
            actor,
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
    let roadmap = data.join("roadmap");
    let store = EventStore::new(roadmap.clone(), prodrome_core::policy::Untrusted::none());
    let items = ["alpha", "bravo", "calendarlinks"];
    let mut terms = vec![fpl::mk_flat(0.6).expect("a term")];
    terms.extend(
        items
            .iter()
            .map(|i| fpl::mk_ref((*i).to_owned()).expect("a ref")),
    );
    let mut events = vec![todo(
        "suzatarytwo",
        "adalogin",
        fpl::mk_conj(terms, -4.0).expect("a conjunction"),
    )];
    events.extend(
        items
            .iter()
            .map(|i| todo(i, "adalogin", fpl::mk_flat(0.7).expect("a term"))),
    );
    events.push(prodrome_core::event::mk_completed("alpha", at, "adalogin", "").expect("an event"));
    for event in events {
        suzatary_store::store::append_to(&store, GENESIS, event).expect("appends");
    }
    let ops = EventStore::new(
        data.join("events-ops"),
        prodrome_core::policy::Untrusted::none(),
    );
    suzatary_store::store::append_to(
        &ops,
        GENESIS,
        todo("opsonly", "ada", fpl::mk_flat(0.7).expect("a term")),
    )
    .expect("appends");
    write(&roadmap, "charter.toml", "public = true\n");
    write(&roadmap, ".gitignore", "/.heads\n/.lock\n/_site\n");
    write(&roadmap, "flake.lock", &pinned("0".repeat(40).as_str()));
    write(
        &roadmap,
        "releases.toml",
        &format!("genesis = \"{}\"\n{PLAN}", genesis_of(&roadmap)),
    );
    run(
        isolated(Command::new(drawing(data)))
            .args(["prodrome", "draw", "--store"])
            .arg(&roadmap)
            .args(["--link", "https://example.test/roadmap/"]),
        "the first draw",
    );
    run_git(
        data,
        &["init", "-q", "--bare", "-b", "master", "roadmap.git"],
    );
    run_git(&roadmap, &["init", "-q", "-b", "master"]);
    run_git(&roadmap, &["remote", "add", "origin", "../roadmap.git"]);
    run_git(&roadmap, &["add", "-A"]);
    run_git(&roadmap, &["commit", "-q", "-m", "The roadmap begins"]);
    run_git(&roadmap, &["push", "-q", "-u", "origin", "master"]);
    std::fs::create_dir_all(data.join("local")).expect("the box-local tier");
    write(data, "local/personal-terms.txt", "zzperson\n");
    write(data, "local/context-terms.txt", "zzcontext\n");
    std::fs::create_dir_all(data.join("scripts")).expect("scripts");
    std::fs::copy(
        repo().join("scripts/secret-rules.toml"),
        data.join("scripts/secret-rules.toml"),
    )
    .expect("the secret rules");
    let path = data.join("instance.toml");
    let file = RELEASE_INSTANCE
        .replace("{data}", &data.display().to_string())
        .replace("{key}", suzatary_connect::cast::keys::root().as_str());
    std::fs::write(&path, file).expect("an instance file");
    path
}

/// The name of the `Genesis` the store at `store` holds.
fn genesis_of(store: &Path) -> String {
    std::fs::read_dir(store.join("objects"))
        .expect("the objects")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| std::fs::read_to_string(path).is_ok_and(|t| t.starts_with("Genesis(")))
        .and_then(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .expect("a genesis")
}

/// A `flake.lock` pinning the application at `rev`, as the roadmap's lock spells it.
fn pinned(rev: &str) -> String {
    format!(
        "{{\n  \"nodes\": {{\n    \"suzatary\": {{\n      \"locked\": {{\n        \"rev\": \"{rev}\"\n      }}\n    }}\n  }}\n}}\n"
    )
}

/// The binary under test, drawing the roadmap's pictures with the design system this test's shell
/// names: a cargo build has no package of its own, and a gate refuses an inherited `TYPST_` name.
fn drawing(data: &Path) -> PathBuf {
    suzatary_testkit::fake_program(
        &data.join("bin"),
        "suzatary-drawing",
        &format!(
            "#!/bin/sh\nTYPST_PACKAGE_PATH='{}' exec '{}' \"$@\"\n",
            std::env::var("TYPST_PACKAGE_PATH").unwrap_or_default(),
            super::binary::path()
        ),
    )
}

/// The stub of `nix flake lock` a land moves the pin with: `<clone> <rev>` pins `rev`.
const PIN: &str = "#!/bin/sh\nprintf '{\\n  \"nodes\": {\\n    \"suzatary\": {\\n      \"locked\": {\\n        \"rev\": \"%s\"\\n      }\\n    }\\n  }\\n}\\n' \"$2\" >\"$1/flake.lock\"\n";

/// What a train run needs to read and write the fixture's roadmap and upkeep chain, and to move
/// the roadmap's pin.
fn release_env(instance: &Path) -> Vec<(&'static str, String)> {
    let data = instance.parent().expect("the instance's directory");
    for dir in ["home", "run"] {
        std::fs::create_dir_all(data.join(dir)).expect("a scratch directory");
    }
    let pin = suzatary_testkit::fake_program(&data.join("bin"), "pin", PIN);
    vec![
        ("SUZATARY_INSTANCE", instance.display().to_string()),
        ("SUZATARY_TRAIN_PIN", pin.display().to_string()),
        ("SUZATARY_TRAIN_BIN", drawing(data).display().to_string()),
        // A login session's, as the box's operator has: the binary's paths live under them.
        ("HOME", data.join("home").display().to_string()),
        ("XDG_RUNTIME_DIR", data.join("run").display().to_string()),
        (
            suzatary_signatures::signatures::BOX_KEY,
            suzatary_connect::cast::keys::ROOT.to_owned(),
        ),
        (
            suzatary_signatures::signatures::AGENT_KEY,
            suzatary_connect::cast::keys::AGENT.to_owned(),
        ),
        (
            suzatary_mail::mail::dedup_key::DEDUP_KEY,
            suzatary_connect::cast::keys::DEDUP.to_owned(),
        ),
    ]
}

/// THE VERSION IS NO COMPILED INPUT: what a release cut writes (the changelog and
/// `app/release.toml`, pinned by `train_releases_what_the_roadmap_reads_and_records_it_there`) is
/// included by no crate's source, and no crate has a build script that could read it, so a
/// train's release moves no crate's inputs and no gate key. The binary reads its release at run
/// time, from the file the flake installs beside it (`suzatary_cli::stamp`), and its plan from
/// beside the roadmap store.
#[test]
fn a_release_cut_changes_no_file_a_crate_compiles() {
    let app = repo().join("app");
    let cut: Vec<PathBuf> = ["CHANGELOG.md", "app/release.toml"]
        .iter()
        .map(|p| repo().join(p))
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    let paths: Vec<PathBuf> = super::includes::includes(&app)
        .iter()
        .map(super::includes::Include::target)
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    assert!(!paths.is_empty(), "the scan finds the crates' includes");
    for path in &paths {
        assert!(!cut.contains(path), "a crate includes {}", path.display());
        assert!(
            !path.ends_with("releases.toml"),
            "a crate compiles a release plan in: {}",
            path.display()
        );
    }
    for crate_ in std::fs::read_dir(&app).expect("app/").flatten() {
        assert!(
            !crate_.path().join("build.rs").exists(),
            "{} has a build script, which could read the release",
            crate_.path().display()
        );
    }
}

/// A tree at `version`.
fn release_files(version: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "app/Cargo.toml",
            "[workspace.package]\nversion = \"0.0.0\"\n".to_owned(),
        ),
        ("app/release.toml", format!("version = \"{version}\"\n")),
    ]
}

/// `git <args>` in the fixture roadmap's origin, its stdout trimmed.
fn roadmap_origin(data: &Path, args: &[&str]) -> String {
    run_git(&data.join("roadmap.git"), args)
}

/// The roadmap's origin's commits naming `rev` in a `Shipped:` trailer.
fn shipped(data: &Path, rev: &str) -> usize {
    roadmap_origin(
        data,
        &[
            "log",
            "--format=%(trailers:key=Shipped,valueonly,separator=%x2C)",
            "master",
        ],
    )
    .lines()
    .filter(|line| *line == rev)
    .count()
}

/// The `Completed` events of `todo` in the store `dir` under `data`, as (actor, note).
fn completions(data: &Path, dir: &str, todo: &str) -> Vec<(String, String)> {
    suzatary_record::record::EventStore::new(
        data.join(dir),
        prodrome_core::policy::Untrusted::none(),
    )
    .events()
    .expect("the store reads")
    .iter()
    .filter_map(|e| match e {
        suzatary_record::record::TodoEvent::Completed(done) if done.todo.as_str() == todo => {
            Some((done.actor.as_str().to_owned(), done.note.clone()))
        }
        _ => None,
    })
    .collect()
}

/// The roadmap's origin's master written out, read as a store: what a visitor reads.
fn published_roadmap(data: &Path, name: &str) -> PathBuf {
    let out = data.join(name);
    let _ = std::fs::remove_dir_all(&out);
    run_git(
        data,
        &[
            "clone",
            "-q",
            "roadmap.git",
            &out.file_name().expect("a name").to_string_lossy(),
        ],
    );
    out
}

/// The todos a PR names complete are recorded done where each is found when it lands, and the
/// train's release is the version the roadmap then reads: cut at build from the roadmap commit
/// its `Tasks:` trailer names, checked at land against the roadmap's head, pushed with its tag in
/// one push, published, and then recorded in ONE roadmap commit holding exactly the `Completes:`
/// todos done, the pin moved to core's publication and the pictures drawn, pushed after door 2.
/// A private todo is completed on the upkeep chain and named nowhere public. Recording again
/// writes nothing.
#[test]
fn train_releases_what_the_roadmap_reads_and_records_it_there() {
    let t = Train::new("release");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_moves(&files);
    let tasks = roadmap_origin(&data, &["rev-parse", "master"]);

    t.pr(
        1,
        "calendar links",
        &[("changelog.d/links.md", "### Added\n- calendar links\n")],
    );
    std::fs::write(
        t.scratch.join("prs/1.body"),
        "Links in the calendar.\n\nCompletes: calendarlinks opsonly\n",
    )
    .expect("a description");
    let built = t.train_with(&["build", "1"], &env);
    assert!(built.status.success(), "build: {}", text(&built.stderr));
    assert!(
        text(&built.stdout).contains("release 1.2.0, completing calendarlinks opsonly"),
        "{}",
        text(&built.stdout)
    );
    assert_eq!(
        t.git(&["log", "-1", "--format=%s", "train"]),
        "release: 1.2.0"
    );
    assert_eq!(
        t.git(&[
            "log",
            "-1",
            "--format=%(trailers:key=Tasks,valueonly)",
            "train"
        ]),
        tasks,
        "the release names the roadmap commit it was cut from"
    );
    assert_eq!(
        t.git(&["diff", "--name-only", "train^", "train"]),
        "CHANGELOG.md\napp/release.toml\nchangelog.d/links.md",
        "a release cut writes the changelog and the release file and removes the fragments, which \
         no crate compiles (a_release_cut_changes_no_file_a_crate_compiles)"
    );
    let changelog = t.git(&["show", "train:CHANGELOG.md"]);
    assert!(
        changelog.contains("## [Unreleased]\n\n## [1.2.0] - ")
            && changelog.contains(
                "- Completes `calendarlinks`: the calendarlinks item\n\n### Added\n- calendar links\n"
            )
            && !changelog.contains("opsonly"),
        "the public changelog names the roadmap's todo and never the private one: {changelog}"
    );
    assert!(
        t.git(&["show", "train:app/release.toml"])
            .contains("completes = [\"calendarlinks\"]"),
        "the code's copy of the reading names the roadmap's todos alone"
    );

    assert!(t.train_with(&["gate"], &env).status.success());
    let landed = t.train_with(&["land"], &env);
    let train = t.git(&["rev-parse", "train"]);
    assert!(landed.status.success(), "land: {}", text(&landed.stderr));
    assert_eq!(t.origin_master(), train);
    assert_eq!(
        run_git(
            &t.scratch.join("origin.git"),
            &["rev-parse", "v1.2.0^{commit}"]
        ),
        train,
        "the tag is the landed release"
    );
    recorded_once(&t, &data, &train);
    let again = t.train_with(&["record"], &env);
    assert!(again.status.success(), "record: {}", text(&again.stderr));
    assert_eq!(shipped(&data, &train), 1, "recording again writes nothing");
    assert_eq!(completions(&data, "events-ops", "opsonly").len(), 1);
}

/// What a visitor reads after the land of `train`, the fixture's release 1.2.0 completing
/// `calendarlinks` (and the private `opsonly`): one roadmap commit naming it, holding exactly one
/// new object and the pin at core's publication, and the private todo done on the upkeep chain.
fn recorded_once(t: &Train, data: &Path, train: &str) {
    let core = t.core(&["rev-parse", "master"]);
    assert_eq!(shipped(data, train), 1, "one roadmap commit records it");
    let head = roadmap_origin(data, &["log", "-1", "--format=%s%n%b", "master"]);
    assert!(
        head.starts_with("Ship 1.2.0") && head.contains("Release: 1.2.0"),
        "{head}"
    );
    assert_eq!(
        roadmap_origin(data, &["show", "master:flake.lock"]),
        pinned(&core).trim(),
        "the pin is core's publication of the release"
    );
    let added = roadmap_origin(
        data,
        &[
            "diff",
            "--name-only",
            "--diff-filter=A",
            "master^",
            "master",
            "--",
            "objects",
        ],
    );
    assert_eq!(added.lines().count(), 1, "exactly one object: {added}");
    let object = roadmap_origin(data, &["show", &format!("master:{}", added.trim())]);
    assert!(
        !object.contains("PR #") && !head.contains("PR #"),
        "the roadmap names nothing of the private repository: {object}{head}"
    );
    let visible = published_roadmap(data, "visible");
    assert_eq!(
        completions(data, "visible", "calendarlinks"),
        vec![("assistant".to_owned(), "the release 1.2.0".to_owned())],
        "a visitor reads it completed"
    );
    for todo in ["bravo", "suzatarytwo", "opsonly"] {
        assert!(
            completions(data, "visible", todo).is_empty(),
            "the roadmap completes exactly the train's: {todo}"
        );
    }
    assert!(visible.join("roadmap.svg").is_file());
    assert_eq!(
        completions(data, "events-ops", "opsonly"),
        vec![("assistant".to_owned(), "PR #1, 1.2.0".to_owned())],
        "the private todo is completed on the upkeep chain"
    );
}

/// A cut is a function of the code tree and the roadmap commit it names: what the box's clone
/// holds and its origin does not (an edit not pushed yet) moves nothing, and a cut from the same
/// commit by the binary alone, with no deployment, is the same release.
#[test]
fn train_cut_reads_the_roadmap_commit_it_names() {
    let t = Train::new("release-tasks");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_releases(&files, "1.1.0");
    // Completing 2.0 in the clone alone, not pushed: no public input names it.
    let closed = crate::binary::keyed()
        .args(["--instance", &instance.to_string_lossy()])
        .args([
            "todo",
            "--chain",
            "roadmap",
            "done",
            "suzatarytwo",
            "--actor",
            "ada",
        ])
        .output()
        .expect("the binary runs");
    assert!(closed.status.success(), "{}", text(&closed.stderr));
    t.pr(1, "a fix", &[("a.txt", "a\n")]);
    let built = t.train_with(&["build", "1"], &env);
    assert!(built.status.success(), "build: {}", text(&built.stderr));
    assert_eq!(
        t.git(&["log", "-1", "--format=%s", "train"]),
        "release: 1.1.1",
        "the clone's unpushed edit moves no version"
    );
    let tasks = t.git(&[
        "log",
        "-1",
        "--format=%(trailers:key=Tasks,valueonly)",
        "train",
    ]);
    let commit = data.join("at-tasks");
    std::fs::create_dir_all(&commit).expect("a directory");
    run(
        Command::new("sh").arg("-c").arg(format!(
            "git -C '{}' archive {tasks} | tar -x -C '{}'",
            data.join("roadmap.git").display(),
            commit.display()
        )),
        "the roadmap commit written out",
    );
    let tree = data.join("tree");
    std::fs::create_dir_all(&tree).expect("a directory");
    run(
        Command::new("sh").arg("-c").arg(format!(
            "git -C '{}' archive origin/master | tar -x -C '{}'",
            t.work().display(),
            tree.display()
        )),
        "origin/master's tree written out",
    );
    let alone = crate::binary::keyed()
        .args(["--instance", "/nonexistent/instance.toml"])
        .args(["release", "cut", "--last", "1.1.0", "--tree"])
        .arg(&tree)
        .arg("--store")
        .arg(&commit)
        .output()
        .expect("the binary runs");
    assert!(alone.status.success(), "{}", text(&alone.stderr));
    assert!(
        text(&alone.stdout).contains("\"version\":\"1.1.1\""),
        "{}",
        text(&alone.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(tree.join("app/release.toml")).expect("the release"),
        t.git(&["show", "train:app/release.toml"]) + "\n",
        "the same tree and commit cut the same release, with no deployment"
    );
}

/// A release completed between build and land is the next train's: the roadmap's head reads above
/// the cut, so the land goes ahead, and the tag is the cut.
#[test]
fn train_land_takes_a_roadmap_that_moved_above_the_cut() {
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
        .args(["--instance", &instance.to_string_lossy()])
        .args([
            "todo",
            "--chain",
            "roadmap",
            "done",
            "calendarlinks",
            "--actor",
            "ada",
        ])
        .output()
        .expect("the binary runs");
    assert!(closed.status.success(), "{}", text(&closed.stderr));
    let pushed = isolated(Command::new(repo().join("scripts/roadmap-push.sh")))
        .arg(data.join("roadmap"))
        .envs(env.iter().copied())
        .env("SUZATARY_BIN", drawing(&data))
        .output()
        .expect("roadmap-push.sh runs");
    assert!(pushed.status.success(), "{}", text(&pushed.stderr));
    assert_eq!(
        roadmap_origin(&data, &["log", "-1", "--format=%B", "master"]),
        "The box's writes",
        "an ordinary push's commit carries the script's own subject and nothing a caller wrote"
    );
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

/// THE BINARY THAT WRITES IS A NAMED COMMIT'S: with no `SUZATARY_TRAIN_BIN`, every app the train
/// asks `scripts/app-bin.sh` for is a commit's, origin/master's at build and the train's at land,
/// or the deployed one, which reads each publication's tree; never the checkout as it is.
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
    let _ = suzatary_testkit::fake_program(
        &app.join("bin"),
        "suzatary",
        &format!("#!/bin/sh\nexec '{}' \"$@\"\n", drawing(&data).display()),
    );
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
    // The deployed app, which reads each publication's tree (train.sh's own app-bin.sh finds it).
    let state = t.scratch.join("state");
    std::fs::create_dir_all(state.join("suzatary/roots")).expect("the roots");
    std::os::unix::fs::symlink(&app, state.join("suzatary/roots/lock-bin")).expect("lock-bin");
    let unpinned = |args: &[&str]| {
        let mut cmd = t.command(args, &env);
        cmd.env_remove("SUZATARY_TRAIN_BIN")
            .env("XDG_STATE_HOME", &state);
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
    assert_eq!(shipped(&data, &train), 1, "{}", text(&landed.stderr));
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
    // A tag of the version the roadmap reads, on a commit no release on master names.
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
    let tasks = roadmap_origin(&data, &["rev-parse", "master"]);
    t.git(&[
        "commit",
        "-q",
        "-m",
        "release: 1.2.0",
        "-m",
        &format!("Release: 1.2.0\nCompletes: calendarlinks\nTasks: {tasks}"),
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
        completions(&data, "roadmap", "calendarlinks").is_empty(),
        "nothing is written for it"
    );
}

/// The count reads origin's newest release alone, and a land cut short before its roadmap commit
/// (its completions refused) is repaired by the next command: a local tag moves nothing, and the
/// next build records what the land did not, once.
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
            "#!/bin/sh\n[ \"$1 $2\" != \"release complete\" ] || [ ! -e '{flag}' ] || {{ echo 'the store is busy' >&2; exit 1; }}\nexec '{real}' \"$@\"\n",
            flag = flag.display(),
            real = drawing(&data).display()
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
    let train = t.git(&["rev-parse", "train"]);
    assert_eq!(
        run_git(
            &t.scratch.join("origin.git"),
            &["rev-parse", "v1.2.0^{commit}"]
        ),
        t.origin_master(),
        "master and the tag landed"
    );
    assert_eq!(shipped(&data, &train), 0, "the record failed");

    // No local v1.2.0, a stale local v1.7.0, and the record no longer failing: the next build
    // counts from origin's tag and records the land's release first.
    std::fs::remove_file(&flag).expect("the flag goes");
    t.git(&["tag", "-d", "v1.2.0"]);
    t.git(&["tag", "-f", "v1.7.0", "origin/master"]);
    let next = t.train_with(&["build", "2"], &env);
    assert!(next.status.success(), "build: {}", text(&next.stderr));
    assert_eq!(shipped(&data, &train), 1, "the next command recorded it");
    published_roadmap(&data, "visible");
    assert_eq!(completions(&data, "visible", "calendarlinks").len(), 1);
    assert_eq!(
        t.git(&["log", "-1", "--format=%s", "train"]),
        "release: 1.2.1",
        "counted from origin's v1.2.0, not the local v1.7.0"
    );
}

/// A land whose roadmap commit was made and not pushed (its origin refusing) leaves the commit in
/// the clone; the next command finds it by its `Shipped:` trailer and pushes it, writing nothing
/// again: each todo is completed once, and one commit records the release.
#[test]
fn train_repairs_a_roadmap_commit_its_land_did_not_push() {
    let t = Train::new("release-unpushed");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let files = release_files("1.1.0");
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
    t.master_moves(&files);
    t.pr(1, "calendar links", &[("a.txt", "a\n")]);
    std::fs::write(t.scratch.join("prs/1.body"), "Completes: calendarlinks\n").expect("a body");
    t.pr(2, "a fix", &[("b.txt", "b\n")]);
    assert!(t.train_with(&["build", "1"], &env).status.success());
    assert!(t.train_with(&["gate"], &env).status.success());
    // The roadmap's origin refuses every push while the flag is there.
    let flag = data.join("refuse-pushes");
    let hook = data.join("roadmap.git/hooks/pre-receive");
    write(
        &data,
        "roadmap.git/hooks/pre-receive",
        &format!("#!/bin/sh\n[ ! -e '{}' ]\n", flag.display()),
    );
    run(Command::new("chmod").arg("+x").arg(&hook), "chmod");
    std::fs::write(&flag, "").expect("the flag");
    let landed = t.train_with(&["land"], &env);
    let train = t.git(&["rev-parse", "train"]);
    assert!(!landed.status.success(), "a land whose push failed says so");
    assert!(
        text(&landed.stderr).contains("not pushed"),
        "{}",
        text(&landed.stderr)
    );
    assert_eq!(t.origin_master(), train, "master landed");
    assert_eq!(shipped(&data, &train), 0, "origin was not written");
    assert_eq!(
        run_git(
            &data.join("roadmap"),
            &["log", "-1", "--format=%(trailers:key=Shipped,valueonly)"]
        ),
        train,
        "the clone holds the commit"
    );

    std::fs::remove_file(&flag).expect("the flag goes");
    let next = t.train_with(&["build", "2"], &env);
    assert!(next.status.success(), "build: {}", text(&next.stderr));
    assert_eq!(shipped(&data, &train), 1, "pushed once");
    published_roadmap(&data, "visible");
    assert_eq!(
        completions(&data, "visible", "calendarlinks").len(),
        1,
        "completed once"
    );
}

/// The roadmap's push refuses a range door 2 refuses: a commit in the clone holding a term the
/// lists name stays in the clone, origin unchanged, and the push names where, never the word. One
/// upkeep todo says where, and the pushes after it, refused the same, file nothing more.
#[test]
fn roadmap_push_refuses_a_range_door_2_refuses() {
    let t = Train::new("roadmap-push-refused");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let roadmap = data.join("roadmap");
    let before = roadmap_origin(&data, &["rev-parse", "master"]);
    write(&roadmap, "notes.md", "a note about zzperson\n");
    run_git(&roadmap, &["add", "notes.md"]);
    run_git(&roadmap, &["commit", "-q", "-m", "a note"]);
    let push = || {
        isolated(Command::new(repo().join("scripts/roadmap-push.sh")))
            .arg(&roadmap)
            .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
            .env("SUZATARY_BIN", drawing(&data))
            .output()
            .expect("roadmap-push.sh runs")
    };
    let out = push();
    let said = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("notes.md:1"), "{said}");
    assert!(!said.contains("zzperson"), "{said}");
    assert_eq!(
        roadmap_origin(&data, &["rev-parse", "master"]),
        before,
        "nothing is pushed"
    );
    let filed = || -> Vec<String> {
        suzatary_record::record::EventStore::new(
            data.join("events-ops"),
            prodrome_core::policy::Untrusted::none(),
        )
        .events()
        .expect("the upkeep chain reads")
        .iter()
        .filter_map(|e| match e {
            suzatary_record::record::TodoEvent::Authored(made)
                if made.todo.as_str().starts_with("doortwo") =>
            {
                Some(made.payload.detail.as_str().to_owned())
            }
            _ => None,
        })
        .collect()
    };
    let [detail] = &filed()[..] else {
        panic!("one upkeep todo: {:?}", filed())
    };
    assert!(
        detail.contains("`notes.md:1`") && !detail.contains("zzperson"),
        "{detail}"
    );
    let again = push();
    assert_eq!(again.status.code(), Some(1), "{}", text(&again.stderr));
    assert_eq!(
        filed().len(),
        1,
        "a refusal of the same hits files nothing more"
    );
}

/// A pass with nothing new costs nothing: a clone holding no new object and no commit its origin
/// lacks returns at once, reaching no remote (its origin here is gone) and drawing nothing.
#[test]
fn roadmap_push_with_nothing_new_reaches_no_remote() {
    let t = Train::new("roadmap-push-quiet");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let roadmap = data.join("roadmap");
    run_git(
        &roadmap,
        &["remote", "set-url", "origin", "/nonexistent/roadmap.git"],
    );
    std::fs::remove_file(roadmap.join("roadmap.svg")).expect("the picture goes");
    run_git(&roadmap, &["checkout", "-q", "--", "roadmap.svg"]);
    let out = isolated(Command::new(repo().join("scripts/roadmap-push.sh")))
        .arg(&roadmap)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .env("SUZATARY_BIN", "/nonexistent/suzatary")
        .output()
        .expect("roadmap-push.sh runs");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("nothing to push"),
        "{}",
        text(&out.stderr)
    );
}

/// An event written to a public store names only a public name: at the mount, `ada` writes under
/// the public name its entry gives, and `bea`, which has none, is refused before the store holds
/// anything, the refusal naming the actor and the chain.
#[test]
fn a_write_by_an_actor_with_no_public_name_is_refused_at_the_mount() {
    let t = Train::new("roadmap-names");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let done = |actor: &str, todo: &str| {
        crate::binary::keyed()
            .args(["--instance", &instance.to_string_lossy()])
            .args(["todo", "--chain", "roadmap", "done", todo, "--actor", actor])
            .output()
            .expect("the binary runs")
    };
    let refused = done("bea", "bravo");
    assert!(!refused.status.success(), "an unnamed actor wrote");
    assert!(
        text(&refused.stderr).contains("chain roadmap is public, and bea has no public name there"),
        "{}",
        text(&refused.stderr)
    );
    assert!(
        completions(&data, "roadmap", "bravo").is_empty(),
        "nothing is stored"
    );
    let taken = done("ada", "bravo");
    assert!(taken.status.success(), "{}", text(&taken.stderr));
    assert_eq!(
        completions(&data, "roadmap", "bravo"),
        vec![("adalogin".to_owned(), String::new())],
        "the event names the public name"
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

    // #2 is written before master moves the lock, so its merge is joined, keeping the train's.
    let lock = |said: &str| format!("{LOCK}# {said}\n");
    t.master_moves(&[
        ("app/Cargo.toml", "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n"),
        ("app/src/lib.rs", ""),
        ("app/Cargo.lock", &lock("base")),
    ]);
    t.pr(2, "calendar links", &[("app/Cargo.lock", &lock("two"))]);
    t.master_moves(&[("app/Cargo.lock", &lock("one"))]);
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

impl Train {
    /// The full gates' runs, one `<flag> <commit>` line each.
    fn full_runs(&self) -> Vec<String> {
        std::fs::read_to_string(self.scratch.join("probes.full"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// A full gate that fails where a selective one passed the same tree is a finding, and nothing
/// lands over it, and no gate after it reads it otherwise.
#[test]
fn train_full_red_beside_a_selective_green_is_a_finding() {
    let t = Train::new("full-finding");
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
        1,
        "only the gate named full ran full (its tree half, red, kept the live half from starting)"
    );
}

/// THE TRAIN'S APP IS THE ONE THE GATE BUILT: `scripts/app-bin.sh` takes a named commit's
/// (`SUZATARY_APP_REV`) from that commit's public tree (`scripts/public-tree.sh --store`), the
/// store path the merge gate's deploy-build and publication.yml build, never from `git+file` of
/// the private repository, and only from the store or a binary cache (`--max-jobs 0`): the box
/// never compiles a commit's app to read its release. `--deployed` is the app deploy.sh linked.
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
        lines[1].starts_with(&format!("nix build {}#suzatary ", tree.display()))
            && lines[1].contains(" --max-jobs 0 "),
        "the public tree's package is fetched, never built: {asked}"
    );

    let deployed = scratch.join("deployed");
    std::fs::create_dir_all(deployed.join("bin")).expect("a deployed app");
    let _ = suzatary_testkit::fake_program(
        &deployed.join("bin"),
        "suzatary",
        "#!/bin/sh\necho deployed\n",
    );
    std::fs::create_dir_all(scratch.join("roots")).expect("the roots");
    std::os::unix::fs::symlink(&deployed, scratch.join("roots/lock-bin")).expect("lock-bin");
    let named = Command::new(checkout.join("scripts/app-bin.sh"))
        .arg("--deployed")
        .env("SUZATARY_REPO", &checkout)
        .env("SUZATARY_ROOTS", scratch.join("roots"))
        .output()
        .expect("app-bin.sh runs");
    assert!(named.status.success(), "{}", text(&named.stderr));
    assert_eq!(
        PathBuf::from(text(&named.stdout).trim()),
        deployed.canonicalize().expect("the deployed app"),
        "the deployed app, resolved"
    );
}

impl Train {
    /// The todos the nightly gate filed on the fixture's chain stored at `events`, by id, with
    /// their details.
    fn nightly_todos_in(data: &Path, events: &str) -> std::collections::BTreeMap<String, String> {
        suzatary_record::record::EventStore::new(
            data.join(events),
            prodrome_core::policy::Untrusted::none(),
        )
        .events()
        .expect("the chain reads")
        .iter()
        .filter_map(|e| match e {
            suzatary_record::record::TodoEvent::Authored(made)
                if made.todo.as_str().starts_with("nightly")
                    && made.actor.as_str() == "assistant" =>
            {
                Some((
                    made.todo.as_str().to_owned(),
                    made.payload.detail.as_str().to_owned(),
                ))
            }
            _ => None,
        })
        .collect()
    }

    /// The todos the nightly gate filed on the fixture's upkeep chain, `ops`, by id.
    fn nightly_todos(data: &Path) -> std::collections::BTreeMap<String, String> {
        Self::nightly_todos_in(data, "events-ops")
    }
}

/// THE NIGHTLY GATE gates origin/master in both halves, full and with the live half's web suite
/// whole (`--nightly`), and a green night files nothing.
#[test]
fn train_nightly_gates_master_whole_and_files_nothing_when_green() {
    let t = Train::new("nightly-green");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let env = release_env(&a_release_deployment(&data));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    t.master_moves(&[("a.txt", "a\n")]);
    let master = t.origin_master();
    let out = t.train_with(&["nightly"], &env);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let runs = t.full_runs();
    assert_eq!(
        runs,
        [
            format!("--no-live {master} --nightly"),
            format!("--only-live {master} --nightly")
        ],
        "both halves of master, nightly"
    );
    assert!(Train::nightly_todos(&data).is_empty());
}

/// A red night runs both halves whatever the first says, files one todo for the tree naming the
/// failing steps on the upkeep chain and on no other (THE ROUTING BOUNDARY), and files it once
/// however many nights find it red.
#[test]
fn train_nightly_red_files_one_todo_naming_the_failing_steps() {
    let t = Train::new("nightly-red");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let env = release_env(&a_release_deployment(&data));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    t.master_moves(&[("broken", "x\n"), ("broken-live", "x\n")]);
    let out = t.train_with(&["nightly"], &env);
    assert!(!out.status.success(), "a red night fails");
    assert_eq!(
        t.full_runs().len(),
        2,
        "the live half ran beside a red tree half"
    );
    let tree = t.git(&["rev-parse", "--short=8", "origin/master^{tree}"]);
    let filed = Train::nightly_todos(&data);
    assert_eq!(
        filed.keys().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from([format!("nightly{tree}")])
    );
    assert!(
        text(&out.stderr).contains(&format!("filed todo:ops/nightly{tree} for stub")),
        "{}",
        text(&out.stderr)
    );
    let master = t.origin_master();
    let evidence = format!(".git/train/logs/evidence/stub-{}", &master[..12]);
    let detail = filed.values().cloned().collect::<String>();
    assert!(
        detail.contains("`pages: a *starred* #scene`") && detail.contains(&format!("`{evidence}`")),
        "the todo names the red scene and the evidence's path, and files as markup: {detail}"
    );
    assert!(
        !detail.contains("the page said"),
        "and never the assertion: {detail}"
    );
    for other in ["events", "roadmap"] {
        assert!(
            Train::nightly_todos_in(&data, other).is_empty(),
            "nothing on {other}"
        );
    }
    let again = t.train_with(&["nightly"], &env);
    assert!(!again.status.success());
    assert_eq!(Train::nightly_todos(&data).len(), 1, "filed once");
}

/// With no `[upkeep]`, a red night files nothing anywhere, says so, and still fails.
#[test]
fn train_nightly_files_nothing_without_an_upkeep_chain() {
    let t = Train::new("nightly-upkeep");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let instance = a_release_deployment(&data);
    let env = release_env(&instance);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let file = std::fs::read_to_string(&instance).expect("the instance");
    std::fs::write(&instance, file.replace("[upkeep]\nchain = \"ops\"\n", "")).expect("rewritten");
    t.master_moves(&[("broken", "x\n")]);
    let unfiled = t.train_with(&["nightly"], &env);
    assert!(
        !unfiled.status.success(),
        "a red night fails with nowhere to file"
    );
    assert!(
        text(&unfiled.stderr).contains("names no [upkeep] chain, so no todo is filed"),
        "{}",
        text(&unfiled.stderr)
    );
    for chain in ["events", "roadmap", "events-ops"] {
        assert!(
            Train::nightly_todos_in(&data, chain).is_empty(),
            "nothing on {chain}"
        );
    }
}

/// The nightly gate waits for a running train and runs once its lock is free, where a train
/// refuses to start beside another and names what holds the lock.
#[test]
fn train_nightly_waits_for_a_running_train() {
    let t = Train::new("nightly-waits");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let env = release_env(&a_release_deployment(&data));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    t.master_moves(&[("a.txt", "a\n")]);
    let lock = t.work().join(".git/train/lock");
    std::fs::create_dir_all(lock.parent().expect("its directory")).expect("the train's state");
    write(&t.work(), ".git/train/lock.holder", "build, pid 1\n");
    let mut holder = Command::new("flock")
        .arg(&lock)
        .args(["sleep", "3"])
        .spawn()
        .expect("flock runs");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let refused = t.train(&["gate"]);
    assert!(!refused.status.success());
    assert!(
        text(&refused.stderr).contains("(build, pid 1)"),
        "{}",
        text(&refused.stderr)
    );
    let out = t.train_with(&["nightly"], &env);
    holder.wait().expect("the holder ends");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("the nightly gate waits for it"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(t.full_runs().len(), 2);
}

/// A red job whose gate printed no "failing steps' last lines" block (an e2e job stopped before its
/// summary) is reported by its failed scenes and the lines under them, its colours removed, so the
/// operator never fetches the job's log.
#[test]
fn train_gate_github_reports_a_red_scene_without_a_block() {
    let t = Train::new("github-e2e-red");
    t.on_github();
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    let (hold, _said) = holding(&t, "hold-live");
    let hold_path = hold.to_str().expect("a UTF-8 path");
    let gating = t
        .command(
            &["gate", "--remote", "github"],
            &[
                ("GH_RUN", "e2e-red"),
                ("GATE_HOLD_LIVE", hold_path),
                ("SUZATARY_TRAIN_POLL", "1"),
            ],
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("train.sh starts");
    let red = ends_within_two_minutes(&t, gating);
    let stderr = text(&red.stderr);
    assert!(!red.status.success(), "{stderr}");
    let report = &stderr[stderr.find("tree half: failed").expect("the report")..];
    assert!(
        report.contains(
            "--- job gate (e2e-1)\n✖ destinations: the inbox opens (240100ms)\n  Error: waited 240000ms for the inbox\n    last: it stayed false\n"
        ) && !report.contains("✔ list")
            && !report.contains("TestContext")
            && !report.contains('\u{1b}'),
        "the report names the failed scene and why: {report}"
    );
    live_half_stopped(&hold);
}

impl Train {
    /// The comments posted on pull request `n`, as the stub `gh` kept them.
    fn comments(&self, n: u32) -> String {
        std::fs::read_to_string(self.scratch.join("comments").join(n.to_string()))
            .unwrap_or_default()
    }

    /// `check <n>` while another `scripts/train.sh` holds the train's lock, the checkout's HEAD
    /// and status unchanged by it.
    fn check_beside_a_train(&self, n: u32) -> Output {
        let lock = self.work().join(".git/train/lock");
        std::fs::create_dir_all(lock.parent().expect("its directory")).expect("the train's state");
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&lock)
            .expect("the lock file opens");
        let mut holder = Command::new("flock")
            .arg(&lock)
            .args(["sleep", "120"])
            .spawn()
            .expect("flock runs");
        assert!(
            within_two_minutes(|| !holders(&lock).is_empty()),
            "the lock is held"
        );
        let (head, status) = (
            self.git(&["rev-parse", "HEAD"]),
            self.git(&["status", "--porcelain"]),
        );
        let out = self.train(&["check", &n.to_string()]);
        let _ = holder.kill();
        let _ = holder.wait();
        assert_eq!(
            self.git(&["rev-parse", "HEAD"]),
            head,
            "the checkout's HEAD is as it was"
        );
        assert_eq!(self.git(&["status", "--porcelain"]), status, "and its tree");
        out
    }
}

/// `check <pr>` needs no lock and leaves the checkout as it is: the PR's head merged on master is
/// checked on GitHub's runners as a public tree on `gate/pr<n>-<head>`, deleted once read, and
/// its green is a comment on the PR. It records no verdict, so no train may land on it.
#[test]
fn train_check_posts_the_runners_green_on_the_pr() {
    let t = Train::new("check-green");
    t.on_github();
    t.pr(
        1,
        "the first",
        &[("a.txt", "a\n"), ("private/notes.md", "private\n")],
    );
    let head = t.head(1);
    let out = t.check_beside_a_train(1);
    assert!(out.status.success(), "check: {}", text(&out.stderr));
    let branch = format!("refs/heads/gate/pr1-{}", &head[..12]);
    let refs = t.core_pushes();
    assert_eq!(refs.len(), 2, "one push and one deletion: {refs:?}");
    let pushed = refs[0]
        .strip_prefix(&format!("{branch} "))
        .unwrap_or_else(|| panic!("only gate/pr1-<head> is pushed: {refs:?}"))
        .to_owned();
    assert_eq!(refs[1], format!("{branch} {ZERO}"));
    let files = t.core(&["ls-tree", "-r", "--name-only", &pushed]);
    assert!(
        files.lines().any(|f| f == "a.txt") && !files.contains("private/"),
        "{files}"
    );
    assert!(
        t.comments(1).contains("is green on GitHub's runners"),
        "{}",
        t.comments(1)
    );
    assert!(
        !t.work().join(".git/train/gates").exists(),
        "no verdict is recorded"
    );
}

/// A red run is posted with the failing lines of its jobs, so the session that wrote the PR reads
/// its own failures.
#[test]
fn train_check_posts_the_runners_red_with_its_failing_lines() {
    let t = Train::new("check-red");
    t.on_github();
    t.pr(1, "the first", &[("broken", "x\n")]);
    let out = t.check_beside_a_train(1);
    assert!(!out.status.success());
    let posted = t.comments(1);
    assert!(
        posted.contains("is red on GitHub's runners") && posted.contains("broken is in the tree"),
        "{posted}"
    );
    assert_eq!(
        t.core(&["for-each-ref", "refs/heads/gate/"]),
        "",
        "no branch is left"
    );
}

/// A merge whose public tree its leak guard refuses never leaves the box: nothing is pushed, and
/// the comment says so without the guard's hits, which name what leaked.
#[test]
fn train_check_of_a_leaking_tree_pushes_nothing_and_posts_no_hit() {
    let t = Train::new("check-leak");
    t.on_github();
    t.pr(1, "the first", &[("leaky.txt", "a leak\n")]);
    let out = t.check_beside_a_train(1);
    assert!(!out.status.success());
    assert_eq!(t.core_pushes(), Vec::<String>::new(), "nothing pushed");
    let posted = t.comments(1);
    assert!(posted.contains("did not pass the leak guard"), "{posted}");
    assert!(
        !posted.contains("leaky"),
        "the hit stays on the box: {posted}"
    );
}

/// The leak guard that decides whether a tree leaves the box reads it with the deployed app, which
/// no tree it reads compiled, so a train cannot weaken the guard over itself, and the box builds no
/// app of the train before the push.
#[test]
fn train_gate_github_reads_its_public_tree_with_the_deployed_app() {
    let t = Train::new("github-deployed-matcher");
    t.on_github();
    t.pr(1, "the first", &[("a.txt", "a\n")]);
    assert!(t.train(&["build", "1"]).status.success());
    let state = t.scratch.join("state");
    let deployed = t.scratch.join("deployed-app");
    std::fs::create_dir_all(deployed.join("bin")).expect("a deployed app");
    std::os::unix::fs::symlink(crate::binary::path(), deployed.join("bin/suzatary"))
        .expect("a link");
    std::fs::create_dir_all(state.join("suzatary/roots")).expect("the roots");
    std::os::unix::fs::symlink(&deployed, state.join("suzatary/roots/lock-bin")).expect("lock-bin");
    let mut cmd = t.command(
        &["gate", "--remote", "github"],
        &[("XDG_STATE_HOME", state.to_str().expect("utf-8"))],
    );
    cmd.env_remove("SUZATARY_TRAIN_BIN");
    let out = cmd.output().expect("train.sh runs");
    assert!(out.status.success(), "gate: {}", text(&out.stderr));
    let matcher =
        std::fs::read_to_string(t.probes_file().with_extension("matcher")).expect("the guard ran");
    let want = deployed
        .canonicalize()
        .expect("the deployed app")
        .join("bin/suzatary");
    assert!(
        !matcher.is_empty() && matcher.lines().all(|bin| Path::new(bin) == want),
        "every leak guard read with the deployed app: {matcher}"
    );
}

/// The live half's early steps red, while the publication run builds: the live half is red, the
/// tree half says nothing (the box builds no public tree and waits on no run of the gate), the
/// runs are cancelled with the branch deleted, and nothing lands.
#[test]
fn train_gate_github_red_early_live_steps_stop_the_tree_half() {
    let t = Train::new("github-live-early-red");
    t.on_github();
    t.pr(1, "the first", &[("broken-live-early", "x\n")]);
    let seed = t.origin_master();
    assert!(t.train(&["build", "1"]).status.success());

    let out = t.train(&["gate", "--remote", "github"]);
    let stderr = text(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("the live half's early steps gave no green, so the tree half is stopped")
            && stderr.contains("live half: failed")
            && stderr.contains("tree half: unknown")
            && stderr.contains("broken-live-early is in the tree"),
        "{stderr}"
    );
    assert!(
        !t.gate_args().iter().any(|a| a.contains("public-tree")),
        "the box builds no public tree: {:?}",
        t.gate_args()
    );
    assert!(
        !t.github_calls()
            .iter()
            .any(|call| call.contains("head_sha")),
        "no gate run is waited on: {:?}",
        t.github_calls()
    );
    assert_eq!(t.core(&["for-each-ref", "refs/heads/gate/"]), "");
    assert!(text(&t.train(&["land"]).stderr).contains("live: failed"));
    assert_eq!(t.origin_master(), seed, "nothing landed");
}

/// With `core` on GitHub, the nightly's tree half runs there on a `gate/nightly-` branch, and the
/// box's runs keep a train's order.
#[test]
fn train_nightly_with_core_on_github_runs_its_tree_half_there() {
    let t = Train::new("nightly-github");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let env = release_env(&a_release_deployment(&data));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    t.on_github();
    t.master_moves(&[("a.txt", "a\n")]);
    let master = t.origin_master();
    let out = t.train_with(&["nightly"], &env);
    let stderr = text(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(stderr.contains("the nightly gate passed"), "{stderr}");
    let branch = format!("refs/heads/gate/nightly-{master} ");
    let refs = t.core_pushes();
    assert!(
        refs.len() == 2 && refs[0].starts_with(&branch) && refs[1].ends_with(ZERO),
        "one push of gate/nightly-<master>, then its deletion: {refs:?}"
    );
    let order: Vec<String> = t
        .gate_args()
        .into_iter()
        .filter(|a| a != "--no-live --steps cloud")
        .map(|a| a.split(' ').take(4).collect::<Vec<_>>().join(" "))
        .collect();
    let at = |prefix: &str| {
        order
            .iter()
            .position(|a| a.starts_with(prefix))
            .unwrap_or_else(|| panic!("{prefix} in {order:?}"))
    };
    assert!(
        at("--nightly --no-live --steps fmt,changelog") < at("dispatch master")
            && at("dispatch master")
                < at("--nightly --only-live --steps live,live-all,live-guards")
            && at("--nightly --only-live --steps live,live-all,live-guards") < at("publication")
            && at("publication") < at("--nightly --no-live --steps public-tree,deployed-shape")
            && at("--nightly --no-live --steps public-tree,deployed-shape")
                < at("--nightly --only-live --steps web-live,deploy-build,push-dry,public-check"),
        "the box's runs, one at a time: {order:?}"
    );
    assert!(Train::nightly_todos(&data).is_empty());
}

/// On GitHub, a red early live run stops neither the runners nor the late live steps, and the
/// night is red; the tree half's green stands beside it.
#[test]
fn train_nightly_with_core_on_github_red_live_stops_nothing() {
    let t = Train::new("nightly-github-red");
    let data = t.scratch.join("data");
    std::fs::create_dir_all(&data).expect("a data root");
    let env = release_env(&a_release_deployment(&data));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    t.on_github();
    t.master_moves(&[("broken-live-early", "x\n")]);
    let out = t.train_with(&["nightly"], &env);
    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "a red night fails: {stderr}");
    assert!(
        t.gate_args()
            .iter()
            .any(|a| a.starts_with("--nightly --only-live --steps web-live")),
        "the late live steps ran: {:?}",
        t.gate_args()
    );
    assert!(
        stderr.contains("live: failed") || stderr.contains("filed todo"),
        "{stderr}"
    );
    assert_eq!(Train::nightly_todos(&data).len(), 1, "{stderr}");
}
