//! The merge gate run in a throwaway checkout with a stub in every step's place: it runs
//! exactly the steps whose inputs moved, keeps a pass only for a clean checkout, hands the
//! machine from one heavy step to the next, and lets one gate at a time into a checkout.

use super::gate_list::{Step, heavies, heavy_order, steps, togethers};
use super::process::{children, runs, until};
use super::scratch::{Scratch, on_path, repo};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The stub run in every step's place: it logs the step's name and fails a step named in
/// `red`. Run where a scene would, it records that each scene it runs fetched what the ignored
/// file `fetches-<scene>` names, as web/e2e/harness.ts records what a scene's tabs fetched, and
/// nothing for a scene the ignored file `unrecorded-<scene>` names. Handed a screenshot
/// directory, it leaves a screenshot there, as a scene does.
const STUB: &str = r#"#!/bin/sh
echo "$1" >>"$GATE_PROBES"
echo "$1 ${SUZATARY_GATE_UNITS:-*}" >>"$GATE_PROBES.units"
[ -z "$SUZATARY_E2E_SHOTS" ] || { mkdir -p "$SUZATARY_E2E_SHOTS" && echo page >"$SUZATARY_E2E_SHOTS/$1.png"; }
if [ -n "$SUZATARY_E2E_READS" ]; then
  mkdir -p "$SUZATARY_E2E_READS"
  for unit in ${SUZATARY_GATE_UNITS:-web/e2e/*.test.ts}; do
    name="$(basename "$unit" .test.ts)"
    [ ! -e "unrecorded-$name" ] || continue
    cat "fetches-$name" >"$SUZATARY_E2E_READS/$name" 2>/dev/null || : >"$SUZATARY_E2E_READS/$name"
  done
fi
! grep -qx "$1" red 2>/dev/null
"#;

/// The fixture's private history, as `.gitattributes` declares one: the public tree, which holds
/// no attributes, has none, so the fixture does not read this repository's.
const PRIVATE: &str =
    "/docs/reports/** export-ignore\n/CHANGELOG.md export-ignore\n/.gitattributes export-ignore\n";

/// A throwaway checkout holding this gate and a file under each path a step may skip.
struct Gate {
    scratch: Scratch,
}

impl Gate {
    fn new() -> Self {
        let gate = Gate {
            scratch: Scratch::new("merge-gate-kept"),
        };
        gate.write(
            "scripts/gate-units.mjs",
            &std::fs::read_to_string(repo().join("scripts/gate-units.mjs")).expect("reads"),
        );
        gate.program(
            "scripts/merge-gate.sh",
            &std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads"),
        );
        gate.write(
            "scripts/groups.sh",
            &std::fs::read_to_string(repo().join("scripts/groups.sh")).expect("reads"),
        );
        for path in [
            "app/secretary/src/lib.rs",
            "web/src/main.ts",
            "web/test/a.test.ts",
            "web/e2e/list.test.ts",
            "web/typst/list.typ",
            "web/static/index.html",
            "android/app.java",
            "docs/plan.md",
            "docs/reports/a-report.md",
            "flake.nix",
            "flake.lock",
            ".gitignore",
        ] {
            gate.write(path, "a line\n");
        }
        gate.write(
            ".gitignore",
            "red\nprobes*\nstub*\nfetches-*\nunrecorded-*\n",
        );
        gate.write(".gitattributes", PRIVATE);
        gate.program("stub", STUB);
        gate.git(&["init", "-q", "-b", "master"]);
        gate.git(&["add", "-A"]);
        gate.git(&["commit", "-q", "-m", "seed"]);
        gate
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.scratch.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(path, content).expect("writes");
    }

    /// An executable file at `rel` holding `content`: a regular file, since the gate commits
    /// the script it runs and keys a stub's passes by the stub's own bytes.
    fn program(&self, rel: &str, content: &str) {
        let path = self.scratch.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        suzatary_testkit::write_program(&path, content.as_bytes());
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(&*self.scratch)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    /// Every change in the checkout, committed.
    fn commit(&self) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", "a change"]);
    }

    /// The tree half with the stub in every step's place: whether it passed, and the steps
    /// that ran.
    fn run(&self, env: &[(&str, &str)]) -> (bool, BTreeSet<String>) {
        self.run_with("stub", env)
    }

    /// The same with the stub named `stub`, a file the checkout ignores.
    fn run_with(&self, stub: &str, env: &[(&str, &str)]) -> (bool, BTreeSet<String>) {
        self.run_args(stub, &["--no-live"], env)
    }

    /// The gate with `args` and the stub named `stub`.
    fn run_args(
        &self,
        stub: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> (bool, BTreeSet<String>) {
        let probes = self.scratch.join("probes");
        let _ = std::fs::remove_file(&probes);
        let _ = std::fs::remove_file(self.scratch.join("probes.units"));
        let out = Command::new(self.scratch.join("scripts/merge-gate.sh"))
            .args(args)
            .current_dir(&*self.scratch)
            .env("SUZATARY_GATE_RUN", self.scratch.join(stub))
            .env("GATE_PROBES", &probes)
            .envs(env.iter().copied())
            .output()
            .expect("the gate runs");
        assert!(
            out.status.code().is_some_and(|c| c <= 1),
            "the gate gave no verdict: {out:?}"
        );
        let ran = std::fs::read_to_string(&probes)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        (out.status.success(), ran)
    }

    /// The units each step of the last run ran, `*` for a step run whole.
    fn units_ran(&self) -> std::collections::BTreeMap<String, BTreeSet<String>> {
        std::fs::read_to_string(self.scratch.join("probes.units"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once(' '))
            .map(|(step, units)| {
                (
                    step.to_owned(),
                    units.split_whitespace().map(str::to_owned).collect(),
                )
            })
            .collect()
    }
}

/// The stub of a gate that holds its lock: it logs the step's name and waits for `go`.
const WAITS: &str = r#"#!/bin/sh
echo "$1" >>"$GATE_PROBES"
while [ ! -e go ]; do sleep 0.1; done
"#;

/// Two gates in one checkout would race on what the steps write outside the tree, so while one
/// runs a second does not start: it exits 75, naming the first, and the first goes on to its
/// verdict.
#[test]
fn merge_gate_a_second_gate_in_one_checkout_does_not_start() {
    let gate = Gate::new();
    gate.program("waits", WAITS);
    let started = gate.scratch.join("started");
    let mut first = Command::new(gate.scratch.join("scripts/merge-gate.sh"))
        .arg("--no-live")
        .current_dir(&*gate.scratch)
        .env("SUZATARY_GATE_RUN", gate.scratch.join("waits"))
        .env("GATE_PROBES", &started)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the first gate starts");
    let first_pid = first.id();
    // The first gate holds its lock from before its first step until it ends.
    until("the first gate runs a step", &[first_pid], || {
        started.exists()
    });

    let second = Command::new(gate.scratch.join("scripts/merge-gate.sh"))
        .arg("--no-live")
        .current_dir(&*gate.scratch)
        .env("SUZATARY_GATE_RUN", gate.scratch.join("stub"))
        .env("GATE_PROBES", gate.scratch.join("probes"))
        .output()
        .expect("the second gate runs");
    gate.write("go", "");
    let first = first.wait().expect("the first gate ends");

    assert_eq!(second.status.code(), Some(75), "{second:?}");
    let said = String::from_utf8_lossy(&second.stderr);
    assert!(
        said.contains(&format!("pid {first_pid}")),
        "the refusal names the first gate: {said}"
    );
    assert!(
        !gate.scratch.join("probes").exists(),
        "the second gate ran no step"
    );
    assert!(
        first.code().is_some_and(|c| c <= 1),
        "the first gate reached its verdict: {first:?}"
    );
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// A pass is kept by what its step reads: a second gate of one tree runs only the unkept
/// steps; a committed change under `web/e2e` runs exactly the steps that read it; a failure is
/// not kept; and a change to the gate or to the environment the steps read runs everything,
/// since both are in every key.
#[test]
fn merge_gate_runs_only_the_steps_whose_inputs_moved() {
    let tree: Vec<Step> = steps().into_iter().filter(|s| s.half == "tree").collect();
    let every: BTreeSet<String> = tree.iter().map(|s| s.name.clone()).collect();
    let unkept: BTreeSet<String> = tree
        .iter()
        .filter(|s| s.reads == "unkept")
        .map(|s| s.name.clone())
        .collect();
    let groups = togethers();
    let units: BTreeSet<String> = tree
        .iter()
        .filter(|s| s.reads.contains('~'))
        .map(|s| s.name.clone())
        .collect();
    // The steps that read `path`, each with every step it is kept together with but a unit
    // step, which runs only its own units whose inputs moved.
    let reading = |path: &str| -> BTreeSet<String> {
        let direct: BTreeSet<String> = tree
            .iter()
            .filter(|s| s.reads_path(path))
            .map(|s| s.name.clone())
            .collect();
        groups
            .iter()
            .filter(|g| g.iter().any(|n| direct.contains(n)))
            .flatten()
            .filter(|n| !units.contains(*n))
            .cloned()
            .chain(direct.iter().cloned())
            .chain(unkept.iter().cloned())
            .collect()
    };
    let gate = Gate::new();

    assert_eq!(
        gate.run(&[]),
        (true, every.clone()),
        "a first gate runs every step"
    );
    assert_eq!(
        gate.run(&[]),
        (true, unkept.clone()),
        "a second gate of one tree"
    );

    gate.write("web/e2e/list.test.ts", "a changed line\n");
    gate.commit();
    let e2e = gate.run(&[]);
    assert_eq!(e2e, (true, reading("web/e2e/list.test.ts")));
    assert_eq!(
        e2e.1,
        names(&[
            "guards",
            "web-test",
            "build-web",
            "e2e",
            "nix-web",
            "nix-module",
            "public-tree",
            "fence-plugin",
            "web-inputs",
            "changelog",
            "deployed-shape",
        ]),
        "a change under web/e2e runs e2e and the steps that read it, and no Rust step but \
         the guards"
    );

    gate.write("red", "nix-module\n");
    gate.write("android/app.java", "a second line\n");
    gate.commit();
    assert_eq!(gate.run(&[]), (false, reading("android/app.java")));
    assert_eq!(
        gate.run(&[]),
        (
            false,
            names(&["nix-module", "fence-plugin", "changelog", "deployed-shape"])
        ),
        "a failure is not kept, so a re-gate runs it again"
    );
    std::fs::remove_file(gate.scratch.join("red")).expect("green again");
    assert_eq!(
        gate.run(&[]),
        (
            true,
            names(&["nix-module", "fence-plugin", "changelog", "deployed-shape"])
        )
    );

    assert_eq!(
        gate.run(&[("CARGO_BUILD_JOBS", "1")]).1,
        every,
        "the environment the steps read is in every key"
    );
    let mut source = std::fs::read_to_string(gate.scratch.join("scripts/merge-gate.sh"))
        .expect("the gate reads");
    source.push_str("# a changed gate\n");
    gate.program("scripts/merge-gate.sh", &source);
    gate.commit();
    assert_eq!(gate.run(&[]).1, every, "the gate is in every key");
}

/// `--steps` runs the named steps of the halves chosen and no other, a heavy one among them
/// included (each heavy step waits on the one named before it that runs, never on one left out);
/// a step it leaves out is neither run nor kept, so a whole gate after it runs that step; and a
/// name that is no step of those halves runs nothing.
#[test]
fn merge_gate_runs_only_the_steps_named() {
    let every: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree")
        .map(|s| s.name)
        .collect();
    let gate = Gate::new();
    assert_eq!(
        gate.run_args(
            "stub",
            &["--no-live", "--steps", "fmt,doc,e2e,changelog"],
            &[]
        ),
        (true, names(&["fmt", "doc", "e2e", "changelog"])),
    );
    gate.write("red", "clippy\n");
    assert_eq!(
        gate.run_args("stub", &["--no-live", "--steps", "clippy"], &[]),
        (false, names(&["clippy"])),
        "a named step's failure is the gate's"
    );
    std::fs::remove_file(gate.scratch.join("red")).expect("green again");
    let rest: BTreeSet<String> = every
        .iter()
        .filter(|s| !["fmt", "doc", "e2e"].contains(&s.as_str()))
        .cloned()
        .collect();
    assert_eq!(
        gate.run(&[]),
        (true, rest),
        "a whole gate runs what the named run left out, and the unkept steps again"
    );

    let probes = gate.scratch.join("probes");
    let _ = std::fs::remove_file(&probes);
    for refused in ["live-guards", "no-such-step"] {
        let out = Command::new(gate.scratch.join("scripts/merge-gate.sh"))
            .args(["--no-live", "--steps", &format!("fmt,{refused}")])
            .current_dir(&*gate.scratch)
            .env("SUZATARY_GATE_RUN", gate.scratch.join("stub"))
            .env("GATE_PROBES", &probes)
            .output()
            .expect("the gate runs");
        assert_eq!(out.status.code(), Some(64), "{refused}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(refused),
            "the refusal names it: {out:?}"
        );
        assert!(!probes.exists(), "and nothing ran");
    }
}

/// A checkout that is not HEAD's tree reads no pass and keeps none: the keys name HEAD's
/// entries, and the guards read HEAD and the index while the other steps read the files. An
/// untracked file is a difference too.
#[test]
fn merge_gate_keeps_nothing_for_a_dirty_checkout() {
    let tree: Vec<Step> = steps().into_iter().filter(|s| s.half == "tree").collect();
    let every: BTreeSet<String> = tree.iter().map(|s| s.name.clone()).collect();
    let reading_e2e: BTreeSet<String> = tree
        .iter()
        .filter(|s| !s.skips().contains(&"web/e2e"))
        .map(|s| s.name.clone())
        .collect();
    let gate = Gate::new();
    assert_eq!(gate.run(&[]).1, every);

    gate.write("web/e2e/list.test.ts", "an uncommitted line\n");
    assert_eq!(gate.run(&[]).1, every, "a dirty checkout reads no pass");
    gate.commit();
    assert_eq!(
        gate.run(&[]).1,
        reading_e2e,
        "and kept none: the tree it ran on, committed, runs every step that reads it"
    );

    gate.write("docs/new.md", "an untracked file\n");
    assert_eq!(gate.run(&[]).1, every, "an untracked file is a difference");
}

/// A red step's screenshots and server output hold a deployment's page text. Under a train they
/// outlive the gate in `$SUZATARY_GATE_EVIDENCE/<step>-<commit>`, which only this user can read,
/// and the summary names the path. A green step's are removed, the newest ten stay (pruned by
/// whole paths, whatever spaces they hold), and the directory is in no pass's key.
#[test]
fn merge_gate_keeps_a_red_steps_evidence_for_this_user_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("exists")
            .permissions()
            .mode()
            & 0o777
    };
    let gate = Gate::new();
    let scratch = Scratch::new("merge-gate-evidence");
    // A path with a space, beside the directory a split at the space would name.
    let evidence = scratch.join("with space").join("evidence");
    let sentinel = scratch.join("with");
    std::fs::create_dir_all(&evidence).expect("dirs");
    std::fs::create_dir_all(&sentinel).expect("dirs");
    for n in 0..10 {
        let old = evidence.join(format!("old-{n}"));
        std::fs::create_dir(&old).expect("dir");
        std::fs::File::open(&old)
            .and_then(|dir| {
                dir.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(n))
            })
            .expect("dated");
    }
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&*gate.scratch)
        .output()
        .expect("git runs");
    let red = evidence.join(format!(
        "e2e-{}",
        &String::from_utf8_lossy(&head.stdout)[..12]
    ));
    let run = |env: &[(&str, &Path)]| {
        Command::new(gate.scratch.join("scripts/merge-gate.sh"))
            .arg("--no-live")
            .current_dir(&*gate.scratch)
            .env("SUZATARY_GATE_RUN", gate.scratch.join("stub"))
            .env("GATE_PROBES", gate.scratch.join("probes"))
            .envs(env.iter().copied())
            .output()
            .expect("the gate runs")
    };

    gate.write("red", "e2e\n");
    let out = run(&[("SUZATARY_GATE_EVIDENCE", &evidence)]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let left: BTreeSet<PathBuf> = std::fs::read_dir(&evidence)
        .expect("reads")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    let mut expected: BTreeSet<PathBuf> =
        (1..10).map(|n| evidence.join(format!("old-{n}"))).collect();
    expected.insert(red.clone());
    assert_eq!(
        left, expected,
        "the red step's is added, and the oldest goes"
    );
    assert!(sentinel.exists(), "the prune removes whole paths alone");
    assert_eq!(mode(&evidence), 0o700);
    assert_eq!(mode(&red), 0o700);
    assert_eq!(mode(&red.join("e2e.png")), 0o600);
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains(&format!("on this box alone: {}", red.display())),
        "the summary names the kept path: {out:?}"
    );

    std::fs::remove_file(gate.scratch.join("red")).expect("removes");
    assert_eq!(
        run(&[("SUZATARY_GATE_EVIDENCE", &evidence)]).status.code(),
        Some(0)
    );
    assert!(!red.exists(), "a green rerun removes the step's evidence");
    assert!(
        !gate.run(&[]).1.contains("e2e"),
        "a run with no evidence directory reads the passes one with it kept"
    );
}

/// The public-tree step leaves out of its key the private history it never reads, read off
/// `.gitattributes`: what it declares `export-ignore`, which `git archive` leaves out, but the
/// changelog the public one is dated from and the attributes themselves, which `git archive`
/// reads to decide; and docs/, memory/ and android/, which none of its builds' sources holds
/// (`merge_gate_web_and_public_builds_read_no_skipped_path`). So a report alone runs it not, and
/// a changelog does.
#[test]
fn merge_gate_public_tree_skips_only_the_private_history() {
    // The public tree holds no attributes, so no private history: its step reads everything.
    let attributes = std::fs::read_to_string(repo().join(".gitattributes")).unwrap_or_default();
    let mut private: Vec<String> = attributes
        .lines()
        .filter_map(|line| line.strip_suffix(" export-ignore"))
        .map(|pattern| {
            pattern
                .trim_start_matches('/')
                .trim_end_matches("/**")
                .to_owned()
        })
        .filter(|path| path != "CHANGELOG.md" && path != ".gitattributes")
        .chain(["docs", "memory", "android"].map(str::to_owned))
        .collect();
    private.sort();
    let step = steps()
        .into_iter()
        .find(|s| s.name == "public-tree")
        .expect("the gate builds the public tree");
    assert_eq!(step.half, "tree", "a function of the tree");
    let mut skips: Vec<String> = step.skips().into_iter().map(str::to_owned).collect();
    skips.sort();
    assert_eq!(skips, private);

    let gate = Gate::new();
    gate.run(&[]);
    gate.write("docs/reports/a-report.md", "a changed line\n");
    gate.commit();
    assert!(
        !gate.run(&[]).1.contains("public-tree"),
        "a report alone is not read"
    );
    gate.write("CHANGELOG.md", "a changed line\n");
    gate.commit();
    assert!(gate.run(&[]).1.contains("public-tree"), "the changelog is");
}

/// A stub's passes are kept apart, keyed by its bytes, so another program in the steps' place,
/// the real gate's included, reads none of them.
#[test]
fn merge_gate_keeps_a_stubs_passes_apart() {
    let every: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree")
        .map(|s| s.name)
        .collect();
    let gate = Gate::new();
    assert_eq!(gate.run(&[]).1, every);
    gate.program("stub-other", &format!("{STUB}# another program\n"));
    assert_eq!(gate.run_with("stub-other", &[]).1, every);
    let kept = gate.scratch.join(".git/gate");
    let tables: Vec<String> = std::fs::read_dir(&kept)
        .expect("the table")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        tables.iter().all(|t| t.starts_with("other-")),
        "a stub's passes sit only under other-<its hash>, never where the real gate reads: \
         {tables:?}"
    );
}

/// The stub of the weights' tests. It logs when each step starts and ends. A heavy step (named
/// in `GATE_HEAVY`) runs until a light step has seen it, and a moment longer, so the other
/// lanes' heavy steps are waiting when it ends; a light step waits until a heavy one runs or
/// every heavy one is done, and logs the heavy steps it saw. Each wait is at most
/// `GATE_WAIT` tenths of a second, so a gate that never runs the two side by side still ends.
const TAKES_TURNS: &str = r#"#!/bin/sh
d="$GATE_PROBES.d"
mkdir -p "$d/running" "$d/seen" "$d/done"
echo "start $1" >>"$GATE_PROBES.times"
n=0
case " $GATE_HEAVY " in
  *" $1 "*)
    touch "$d/running/$1"
    while [ ! -e "$d/seen/$1" ] && [ $n -lt "$GATE_WAIT" ]; do sleep 0.1; n=$((n + 1)); done
    [ "$GATE_WAIT" -eq 0 ] || sleep 0.2
    rm "$d/running/$1"
    touch "$d/done/$1"
    ;;
  *)
    while [ $n -lt "$GATE_WAIT" ] && [ "$(ls "$d/done" | wc -l)" -lt "$(echo $GATE_HEAVY | wc -w)" ]; do
      for h in "$d"/running/*; do
        if [ -e "$h" ]; then
          echo "$1 ${h##*/}" >>"$GATE_PROBES.beside"
          touch "$d/seen/${h##*/}"
          n=$GATE_WAIT
        fi
      done
      sleep 0.1
      n=$((n + 1))
    done
    ;;
esac
echo "end $1" >>"$GATE_PROBES.times"
"#;

/// Two steps of one run, as the stub logged them.
type Pair = (String, String);

/// The tree half under the turn-taking stub, with `env` and the gate's heavy steps named in
/// `order`: the steps that ran beside another, as (a step, the step running when it started),
/// the (light, heavy) pairs that ran side by side, and the steps in the order they started.
fn take_turns(env: &[(&str, &str)], order: &[&str]) -> (Vec<Pair>, BTreeSet<Pair>, Vec<String>) {
    let gate = Gate::new();
    let source = std::fs::read_to_string(gate.scratch.join("scripts/merge-gate.sh"))
        .expect("the gate reads");
    let source: Vec<String> = source
        .lines()
        .map(|line| {
            if line.starts_with("heavy ") {
                format!("heavy {}", order.join(" "))
            } else {
                line.to_owned()
            }
        })
        .collect();
    gate.program("scripts/merge-gate.sh", &(source.join("\n") + "\n"));
    gate.commit();
    gate.program("stub-turns", TAKES_TURNS);
    let heavy = order.join(" ");
    let mut env = env.to_vec();
    env.push(("GATE_HEAVY", &heavy));
    assert!(
        gate.run_with("stub-turns", &env).0,
        "the stub passes every step"
    );
    let probes = gate.scratch.join("probes");
    let times = std::fs::read_to_string(probes.with_extension("times")).expect("the times");
    let mut running: Vec<String> = Vec::new();
    let mut beside = Vec::new();
    let mut started = Vec::new();
    for line in times.lines() {
        match line.split_once(' ') {
            Some(("start", step)) => {
                beside.extend(running.iter().map(|r| (step.to_owned(), r.clone())));
                running.push(step.to_owned());
                started.push(step.to_owned());
            }
            Some(("end", step)) => running.retain(|r| r != step),
            _ => panic!("a line the stub does not write: {line}"),
        }
    }
    assert!(
        running.is_empty(),
        "every step that started ended: {running:?}"
    );
    let seen = std::fs::read_to_string(probes.with_extension("beside"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(light, heavy)| (light.to_owned(), heavy.to_owned()))
        .collect();
    (beside, seen, started)
}

/// The tree half's heavy steps in the order the gate names them.
fn tree_heavy_order() -> Vec<String> {
    let tree: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree")
        .map(|s| s.name)
        .collect();
    heavy_order()
        .into_iter()
        .filter(|h| tree.contains(h))
        .collect()
}

/// Weighted lanes, the default: two heavy steps never run side by side, they run in the order
/// they are named, and a light step runs beside a heavy one. The order holds when it is not the
/// order the steps reach the lock in: the lanes start build, web-inputs and public-tree
/// together, in that order, and named the other way round they still run the other way round.
#[test]
fn merge_gate_runs_one_heavy_step_at_a_time_beside_light_ones() {
    let heavy = heavies();
    let named = tree_heavy_order();
    let named: Vec<&str> = named.iter().map(String::as_str).collect();
    let mut reversed = named.clone();
    let first = ["build", "web-inputs", "public-tree"];
    assert_eq!(reversed[..3], first, "the order this test reverses");
    reversed[..3].reverse();
    for order in [&named, &reversed] {
        let (_, _, started) = take_turns(&[("GATE_WAIT", "50")], order);
        let ran: Vec<&str> = started
            .iter()
            .map(String::as_str)
            .filter(|s| heavy.contains(*s))
            .collect();
        assert_eq!(
            &ran, order,
            "the heavy steps take turns in the order they are named"
        );
    }
    let (beside, seen, _) = take_turns(&[("GATE_WAIT", "50")], &named);
    let both: Vec<&Pair> = beside
        .iter()
        .filter(|(a, b)| heavy.contains(a) && heavy.contains(b))
        .collect();
    assert!(
        both.is_empty(),
        "two heavy steps ran side by side: {both:?}"
    );
    assert!(
        !seen.is_empty(),
        "no light step ran beside a heavy one: the lanes did not run side by side"
    );
    assert!(
        seen.iter()
            .all(|(light, heavy_step)| !heavy.contains(light) && heavy.contains(heavy_step))
    );
}

/// `SUZATARY_GATE_SERIAL=1`, the fallback: one step at a time.
#[test]
fn merge_gate_serial_runs_one_step_at_a_time() {
    let named = tree_heavy_order();
    let named: Vec<&str> = named.iter().map(String::as_str).collect();
    let (beside, seen, _) =
        take_turns(&[("GATE_WAIT", "0"), ("SUZATARY_GATE_SERIAL", "1")], &named);
    assert!(beside.is_empty(), "steps ran side by side: {beside:?}");
    assert!(seen.is_empty(), "{seen:?}");
}

/// A heavy step that fails still hands the machine on: every step after it runs, and the gate
/// reaches its verdict.
#[test]
fn merge_gate_a_failed_heavy_step_hands_the_machine_on() {
    let every: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree")
        .map(|s| s.name)
        .collect();
    let first = tree_heavy_order().remove(0);
    let gate = Gate::new();
    gate.write("red", &format!("{first}\n"));
    assert_eq!(gate.run(&[]), (false, every));
}

/// The stub of a gate whose heavy steps hold the machine: it logs each step's name, and a heavy
/// step (named in `GATE_HEAVY`) waits for the file `GATE_GO`.
const HOLDS: &str = r#"#!/bin/sh
echo "$1" >>"$GATE_PROBES"
case " $GATE_HEAVY " in
  *" $1 "*) while [ ! -e "$GATE_GO" ]; do sleep 0.1; done ;;
esac
"#;

/// Two gates in two worktrees of one repository share the heavy lock: while the first gate's
/// heavy step runs, the second's waits for it, and both reach their verdict once it ends.
#[test]
fn merge_gate_two_worktrees_share_the_heavy_lock() {
    let gate = Gate::new();
    gate.program("stub-holds", HOLDS);
    let stub = gate.scratch.join("stub-holds");
    let other = Scratch::new("merge-gate-worktree");
    let tree = other.join("tree");
    gate.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        &tree.to_string_lossy(),
        "HEAD",
    ]);
    let heavy = tree_heavy_order().join(" ");
    let go = gate.scratch.join("go");
    let spawn = |dir: &Path, probes: &Path, err: std::process::Stdio| {
        Command::new(dir.join("scripts/merge-gate.sh"))
            .arg("--no-live")
            .current_dir(dir)
            .env("SUZATARY_GATE_RUN", &stub)
            .env("GATE_PROBES", probes)
            .env("GATE_HEAVY", &heavy)
            .env("GATE_GO", &go)
            .stdout(std::process::Stdio::null())
            .stderr(err)
            .spawn()
            .expect("a gate starts")
    };
    let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();

    let first_probes = gate.scratch.join("probes-first");
    let mut first = spawn(&gate.scratch, &first_probes, std::process::Stdio::null());
    let holding = tree_heavy_order().remove(0);
    until("the first gate's heavy step runs", &[first.id()], || {
        read(&first_probes).lines().any(|l| l == holding)
    });

    let second_probes = gate.scratch.join("probes-second");
    let second_err = gate.scratch.join("probes-second.err");
    let mut second = spawn(
        &tree,
        &second_probes,
        std::fs::File::create(&second_err).expect("its log").into(),
    );
    until(
        "the second gate's heavy step waits",
        &[first.id(), second.id()],
        || read(&second_err).contains(&format!("waits for the heavy step {holding}")),
    );
    let ran: Vec<String> = read(&second_probes).lines().map(str::to_owned).collect();
    assert!(
        !ran.iter().any(|step| heavy.split(' ').any(|h| h == step)),
        "no heavy step of the second gate ran beside the first's: {ran:?}"
    );

    gate.write("go", "");
    let first = first.wait().expect("the first gate ends");
    let second = second.wait().expect("the second gate ends");
    assert!(first.success() && second.success(), "{first:?} {second:?}");
    let every: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree")
        .map(|s| s.name)
        .collect();
    for probes in [&first_probes, &second_probes] {
        let ran: BTreeSet<String> = read(probes).lines().map(str::to_owned).collect();
        assert_eq!(ran, every, "each gate ran every step");
    }
}

/// The stub of the stop's test: the step named `GATE_HOLD_STEP` writes its pid and waits on the
/// fifo `GATE_HOLD`; every other step passes.
const HELD: &str = r#"#!/bin/sh
[ "$1" = "$GATE_HOLD_STEP" ] || exit 0
echo $$ >"$GATE_HOLD.pid"
cat "$GATE_HOLD" >/dev/null
"#;

/// A TERM to the gate's pid alone stops every lane: the step a lane runs, in a lane beside others,
/// in the quiet lane, and in a serial gate's lane, is gone once the gate ended, and so is the
/// gate's lock.
#[test]
fn merge_gate_stopped_by_term_stops_its_lanes() {
    for (step, env) in [
        ("fmt", None),
        ("e2e", None),
        ("fmt", Some(("SUZATARY_GATE_SERIAL", "1"))),
    ] {
        let gate = Gate::new();
        gate.program("stub-held", HELD);
        let hold = gate.scratch.join("hold");
        let made = Command::new("mkfifo").arg(&hold).status().expect("mkfifo");
        assert!(made.success());
        let mut cmd = Command::new(gate.scratch.join("scripts/merge-gate.sh"));
        cmd.args(["--no-live"])
            .current_dir(&*gate.scratch)
            .env("SUZATARY_GATE_RUN", gate.scratch.join("stub-held"))
            .env("GATE_PROBES", gate.scratch.join("probes"))
            .env("GATE_HOLD", &hold)
            .env("GATE_HOLD_STEP", step)
            .envs(env)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut running = cmd.spawn().expect("the gate runs");
        let pid = hold.with_extension("pid");
        let pid_of_waiting = || {
            std::fs::read_to_string(&pid)
                .ok()
                .filter(|p| p.ends_with('\n') && !children(p.trim()).is_empty())
        };
        until(
            &format!("{step}: the held step runs"),
            &[running.id()],
            || pid_of_waiting().is_some(),
        );
        let waiting = pid_of_waiting().expect("the held step waits on its fifo");
        let step_pid = waiting.trim().to_owned();
        let parts: Vec<String> = std::iter::once(step_pid.clone())
            .chain(children(&step_pid))
            .collect();
        let killed = Command::new("kill")
            .args(["-TERM", &running.id().to_string()])
            .status()
            .expect("kill runs");
        assert!(killed.success());
        let (said, ended) = std::sync::mpsc::channel();
        std::thread::spawn(move || said.send(running.wait()));
        let status = ended
            .recv_timeout(std::time::Duration::from_secs(120))
            .unwrap_or_else(|_| panic!("{step}: the gate ended within two minutes"))
            .expect("the gate ends");
        assert!(!status.success(), "{step}: a stopped gate is not green");
        let left: Vec<&String> = parts.iter().filter(|p| runs(p)).collect();
        assert!(
            left.is_empty(),
            "{step} {env:?}: its step is stopped: {left:?}"
        );
        let lock = gate.scratch.join(".git/merge-gate.lock");
        let free = Command::new("flock")
            .args(["-n"])
            .arg(&lock)
            .arg("true")
            .status()
            .is_ok_and(|s| s.success());
        assert!(
            free,
            "{step} {env:?}: the gate's lock is free once it ended"
        );
    }
}

/// How the lanes share the machine changes no verdict, so it is in no key: a serial gate after a
/// weighted one of the same tree runs only the unkept steps, and a value other than 1 is refused
/// before any step runs.
#[test]
fn merge_gate_keys_no_lane_mode() {
    let unkept: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree" && s.reads == "unkept")
        .map(|s| s.name)
        .collect();
    let gate = Gate::new();
    assert!(gate.run(&[]).0);
    assert_eq!(gate.run(&[("SUZATARY_GATE_SERIAL", "1")]), (true, unkept));

    std::fs::remove_file(gate.scratch.join("probes")).expect("the last run's probes");
    let out = Command::new(gate.scratch.join("scripts/merge-gate.sh"))
        .arg("--no-live")
        .current_dir(&*gate.scratch)
        .env("SUZATARY_GATE_RUN", gate.scratch.join("stub"))
        .env("GATE_PROBES", gate.scratch.join("probes"))
        .env("SUZATARY_GATE_SERIAL", "yes")
        .output()
        .expect("the gate runs");
    assert_eq!(out.status.code(), Some(64), "{out:?}");
    assert!(!gate.scratch.join("probes").exists(), "no step ran");
}

/// A failing reader runs its writers again: with e2e red, a re-gate of the same tree runs
/// web-inputs and build-web beside it, never e2e alone over a bundle a kept build-web left. A unit
/// step whose units all passed (web-test) is not run again: a kept unit reads nothing.
#[test]
fn merge_gate_never_keeps_a_writer_without_its_reader() {
    let gate = Gate::new();
    gate.run(&[]);
    gate.write("red", "e2e\n");
    gate.write("web/src/main.ts", "a changed line\n");
    gate.commit();
    let again = names(&[
        "build-web",
        "e2e",
        "web-inputs",
        "fence-plugin",
        "changelog",
        "deployed-shape",
    ]);
    assert!(!gate.run(&[]).0);
    assert_eq!(
        gate.run(&[]),
        (false, again.clone()),
        "e2e failed, so its writers run with it again"
    );
    std::fs::remove_file(gate.scratch.join("red")).expect("green again");
    assert_eq!(gate.run(&[]), (true, again));
    assert_eq!(
        gate.run(&[]),
        (
            true,
            names(&["fence-plugin", "changelog", "deployed-shape"])
        ),
        "kept together once all passed"
    );
}

/// An ignored file a step reads moves every key: a new proptest counterexample changes the cases
/// `test` runs; and so does the time zone, which the CLI reads through the local clock.
#[test]
fn merge_gate_keys_the_ignored_counterexamples_and_the_time_zone() {
    let every: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree")
        .map(|s| s.name)
        .collect();
    let gate = Gate::new();
    gate.write(".gitignore", "red\nprobes*\nstub*\nproptest-regressions/\n");
    gate.commit();
    gate.run(&[]);
    gate.write("app/api/proptest-regressions/sync.txt", "cc 0123\n");
    assert_eq!(
        gate.run(&[]).1,
        every,
        "a new counterexample runs every step"
    );
    assert_eq!(
        gate.run(&[("TZ", "Pacific/Auckland")]).1,
        every,
        "so does another time zone"
    );
}

/// A throwaway workspace of five members (`suzatary-stores`, `suzatary-secretary` on it,
/// `suzatary-io-sources` on that, `suzatary` on `io-sources`, `suzatary-wasm` on the stores)
/// and test files over two modules, written into `gate`.
/// As in a fresh checkout, the generated wire that `b.test.ts` and the app import is not on disk.
fn units_fixture(gate: &Gate) {
    gate.write(
        "app/Cargo.toml",
        "[workspace]\nresolver = \"3\"\nmembers = [\"stores\", \"secretary\", \"io-local\", \"io-typeset\", \"io-sources\", \"cli\", \"wasm\"]\n",
    );
    for (dir, name, dep) in [
        ("stores", "suzatary-stores", ""),
        (
            "secretary",
            "suzatary-secretary",
            "suzatary-stores = { path = \"../stores\" }\n",
        ),
        ("io-local", "suzatary-io-local", ""),
        ("io-typeset", "suzatary-io-typeset", ""),
        (
            "io-sources",
            "suzatary-io-sources",
            "suzatary-secretary = { path = \"../secretary\" }\n",
        ),
        (
            "cli",
            "suzatary",
            "suzatary-io-sources = { path = \"../io-sources\" }\n",
        ),
        (
            "wasm",
            "suzatary-wasm",
            "suzatary-stores = { path = \"../stores\" }\n",
        ),
    ] {
        gate.write(
            &format!("app/{dir}/Cargo.toml"),
            &format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{dep}"
            ),
        );
        gate.write(&format!("app/{dir}/src/lib.rs"), "a line\n");
    }
    gate.write("web/src/a.ts", "export const a = 1\n");
    gate.write("web/src/b.ts", "export const b = 2\n");
    // The app: a module it loads, a chunk it loads by a dynamic import, and the other bundles.
    gate.write("web/src/wire/http.ts", "export const http = 1\n");
    gate.write(
        "web/src/main.ts",
        "import { a } from \"./a\"\nimport { decode } from \"./wire/generated/payload\"\nconsole.log(a, decode)\nvoid import(\"./later\").then(m => console.log(m.later))\n",
    );
    gate.write("web/src/later.ts", "export const later = 1\n");
    for bundle in ["boot", "sw", "typst/worker", "prodrome/worker"] {
        gate.write(&format!("web/src/{bundle}.ts"), "console.log(1)\n");
    }
    gate.write("app/pages/src/lib.rs", "a page\n");
    gate.write(
        "web/test/a.test.ts",
        "import { a } from \"../src/a\"\nconsole.log(a)\n",
    );
    gate.write(
        "web/test/b.test.ts",
        "import { b } from \"../src/b\"\nimport { decode } from \"../src/wire/generated/payload\"\nconsole.log(b, decode)\n",
    );
    gate.write("web/e2e/harness.ts", "export const h = 1\n");
    gate.write(
        "web/e2e/list.test.ts",
        "import { h } from \"./harness\"\nconsole.log(h)\n",
    );
    gate.write(
        "web/e2e/two.test.ts",
        "import { h } from \"./harness\"\nimport { cast } from \"../test/cast\"\nconsole.log(h, cast, 2)\n",
    );
    gate.write("web/test/cast.ts", "export const cast = \"Ada\"\n");
    gate.commit();
}

/// A unit step runs only the units whose inputs moved: a member and the members that depend on
/// it, read off cargo metadata; a test file whose imports reach the changed module; a scene
/// alone when only its own file moved, and every scene when the app's modules moved, since each
/// drives the whole bundle.
#[test]
fn merge_gate_runs_only_the_units_whose_inputs_moved() {
    if !on_path("esbuild") || !on_path("cargo") || !on_path("node") {
        eprintln!("merge_gate_runs_only_the_units_whose_inputs_moved: no esbuild, cargo or node");
        return;
    }
    let gate = Gate::new();
    units_fixture(&gate);
    let set = |list: &[&str]| names(list);
    assert!(gate.run(&[]).0);
    assert_eq!(
        gate.units_ran()["test"],
        set(&["*"]),
        "a first gate runs every unit"
    );

    gate.write("app/io-sources/src/lib.rs", "a changed line\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    assert_eq!(
        gate.units_ran()["test"],
        set(&["suzatary-io-sources", "suzatary"]),
        "the member and the member that depends on it, never the stores, the secretary or the wasm module"
    );
    assert!(!gate.units_ran().contains_key("web-test"));

    gate.write("web/src/b.ts", "export const b = 3\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    let ran = gate.units_ran();
    assert_eq!(ran["web-test"], set(&["web/test/b.test.ts"]));
    assert_eq!(
        ran["e2e"],
        set(&["*"]),
        "every scene drives the whole bundle"
    );
    assert!(!ran.contains_key("test"), "no Rust unit reads web/src");

    gate.write("web/src/wire/http.ts", "export const http = 2\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    // b.test.ts imports the generated wire, so it reads all of web/src/wire.
    assert_eq!(gate.units_ran()["web-test"], set(&["web/test/b.test.ts"]));

    gate.write(
        "web/e2e/two.test.ts",
        "import { h } from \"./harness\"\nimport { cast } from \"../test/cast\"\nconsole.log(h, cast, 3)\n",
    );
    gate.commit();
    assert!(gate.run(&[]).0);
    let ran = gate.units_ran();
    assert_eq!(ran["e2e"], set(&["web/e2e/two.test.ts"]));
    assert!(!ran.contains_key("web-test"), "no unit test reads a scene");

    gate.write("web/test/cast.ts", "export const cast = \"Grace\"\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    let ran = gate.units_ran();
    assert_eq!(
        ran["e2e"],
        set(&["web/e2e/two.test.ts"]),
        "a scene reads the test file it imports"
    );
    assert!(!ran.contains_key("web-test"), "no unit test imports it");

    gate.write(
        "app/wasm/Cargo.toml",
        "[package]\nname = \"suzatary-wasm\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nsuzatary-stores = { path = \"../stores\" }\n\n[features]\nmore = []\n",
    );
    gate.commit();
    assert!(gate.run(&[]).0);
    assert_eq!(
        gate.units_ran()["test"],
        set(&["*"]),
        "a member's manifest is every unit's input"
    );

    gate.write("web/e2e/harness.ts", "export const h = 2\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    assert_eq!(
        gate.units_ran()["e2e"],
        set(&["*"]),
        "both scenes import the harness"
    );
}

/// `--full` runs every step whole and reads no pass, and keeps the passes it makes.
#[test]
fn merge_gate_full_runs_every_step_whole_and_reads_no_pass() {
    if !on_path("esbuild") || !on_path("cargo") || !on_path("node") {
        eprintln!(
            "merge_gate_full_runs_every_step_whole_and_reads_no_pass: no esbuild, cargo or node"
        );
        return;
    }
    let gate = Gate::new();
    units_fixture(&gate);
    let set = |list: &[&str]| names(list);
    assert!(gate.run(&[]).0);

    let every: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree")
        .map(|s| s.name)
        .collect();
    let (green, ran) = gate.run_args("stub", &["--full", "--no-live"], &[]);
    assert!(green);
    assert_eq!(ran, every, "a full gate reads no pass");
    assert!(
        gate.units_ran().values().all(|units| units == &set(&["*"])),
        "and runs every step whole: {:?}",
        gate.units_ran()
    );
    assert_eq!(
        gate.run(&[]).1,
        steps()
            .into_iter()
            .filter(|s| s.half == "tree" && s.reads == "unkept")
            .map(|s| s.name)
            .collect(),
        "the full gate kept its passes"
    );
}

/// A SCENE IS KEPT BY WHAT IT FETCHED: its imports, every module of the app as it loads, and the
/// chunks and the Rust pages' crates its tabs fetched. A change to a chunk no scene fetched runs
/// no scene; once one fetched it, a change to it runs that scene and no other, and a scene that
/// fetched the Rust pages' module reruns when their crates move. A scene that records nothing
/// (another part ran it) keeps no pass.
#[test]
fn merge_gate_a_scene_is_kept_by_what_it_fetched() {
    if !on_path("esbuild") || !on_path("cargo") || !on_path("node") {
        eprintln!("merge_gate_a_scene_is_kept_by_what_it_fetched: no esbuild, cargo or node");
        return;
    }
    let gate = Gate::new();
    units_fixture(&gate);
    let e2e = |gate: &Gate| gate.units_ran().get("e2e").cloned();
    assert!(gate.run(&[]).0);
    assert_eq!(
        e2e(&gate),
        Some(names(&["*"])),
        "a first gate runs every scene"
    );

    gate.write("web/src/later.ts", "export const later = 2\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    assert_eq!(e2e(&gate), None, "no scene fetched the chunk");

    gate.write("fetches-list", "web/src/later.ts\n");
    gate.write("fetches-two", "app/pages\napp/viewtree\n");
    gate.write("web/e2e/harness.ts", "export const h = 3\n");
    gate.commit();
    assert!(
        gate.run(&[]).0,
        "both scenes run, and record what they fetched"
    );

    gate.write("web/src/later.ts", "export const later = 3\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    assert_eq!(
        e2e(&gate),
        Some(names(&["web/e2e/list.test.ts"])),
        "the scene that fetched the chunk, and no other"
    );

    gate.write("app/pages/src/lib.rs", "another page\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    assert_eq!(
        e2e(&gate),
        Some(names(&["web/e2e/two.test.ts"])),
        "the scene that fetched the Rust pages' module"
    );

    gate.write("web/src/a.ts", "export const a = 9\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    assert_eq!(
        e2e(&gate),
        Some(names(&["*"])),
        "a module the app loads is every scene's"
    );

    gate.write("unrecorded-list", "");
    gate.write("web/e2e/harness.ts", "export const h = 4\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    assert!(gate.run(&[]).0);
    assert_eq!(
        e2e(&gate),
        Some(names(&["web/e2e/list.test.ts"])),
        "the scene that recorded nothing runs again, and the one that did keeps its pass"
    );
}

/// THE STORES' DIGEST KEYS THE KEPT LIVE STEPS ALONE: `SUZATARY_GATE_DATA`, which the train hands
/// the live half, is in the key of every kept live step and of no other, so a live half at the
/// same stores keeps its passes, one at other stores runs its kept steps again, and the tree half
/// keeps its passes whatever the digest.
#[test]
fn merge_gate_keys_the_kept_live_steps_by_the_stores_digest() {
    let gate = Gate::new();
    let live =
        |digest: &str| gate.run_args("stub", &["--only-live"], &[("SUZATARY_GATE_DATA", digest)]);
    let (green, every) = live(&"a".repeat(40));
    assert!(green && !every.is_empty(), "the live half runs: {every:?}");
    let unkept = live(&"a".repeat(40)).1;
    assert!(
        unkept.len() < every.len(),
        "the same stores keep the kept steps' passes"
    );
    assert_eq!(
        live(&"b".repeat(40)).1,
        every,
        "other stores run the kept steps again"
    );
    assert_eq!(live(&"b".repeat(40)).1, unkept);

    let tree = gate.run(&[]).1;
    let tree_unkept = gate.run(&[]).1;
    assert!(tree_unkept.len() < tree.len());
    assert_eq!(
        gate.run(&[("SUZATARY_GATE_DATA", &"c".repeat(40))]).1,
        tree_unkept,
        "the digest moves no tree step's key"
    );
}
