//! The merge gate's two halves (`scripts/merge-gate.sh`): every test that reads the owner's
//! data tree is in the live half, so the tree half, which runs where the data never is (a
//! Codespace) or with it hidden, leaves nothing unread that only the live half would read.
//! And its kept passes: a step whose inputs did not move is not run again, so each step's
//! declared reads must cover what it reads, and the gate run with a stub in every step's place
//! runs exactly the steps whose inputs moved.

use super::{Scratch, repo};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One step of `scripts/merge-gate.sh --list`.
pub(super) struct Step {
    pub(super) half: String,
    /// The steps of one half and lane run one after another; the lanes run side by side.
    lane: String,
    pub(super) name: String,
    /// `all`, `all-but:<path>,…`, `only:<path>,…` or `unkept`; a unit step's, `crates` or
    /// `files:<dir>`, then `~` and its reads as one unit.
    reads: String,
    command: String,
}

impl Step {
    /// The reads the step is kept by as one unit: a unit step's after its `~`.
    fn whole(&self) -> &str {
        self.reads
            .split_once('~')
            .map_or(self.reads.as_str(), |(_, whole)| whole)
    }

    /// The paths the step declares it never reads, as one unit.
    fn skips(&self) -> Vec<&str> {
        self.whole()
            .strip_prefix("all-but:")
            .map(|paths| paths.split(',').collect())
            .unwrap_or_default()
    }

    /// The paths an `only:` step reads, beside the gate's own scripts and flake.
    fn only(&self) -> Option<Vec<&str>> {
        self.whole()
            .strip_prefix("only:")
            .map(|paths| paths.split(',').collect())
    }

    /// Whether a kept step, as one unit, reads `path`.
    fn reads_path(&self, path: &str) -> bool {
        match self.only() {
            Some(only) => ["scripts", "flake.nix", "flake.lock"]
                .iter()
                .chain(only.iter())
                .any(|p| path == *p || path.starts_with(&format!("{p}/"))),
            None => self.reads != "unkept" && !self.skips().iter().any(|p| path.starts_with(p)),
        }
    }
}

pub(super) fn steps() -> Vec<Step> {
    let out = Command::new(repo().join("scripts").join("merge-gate.sh"))
        .arg("--list")
        .output()
        .expect("merge-gate.sh runs");
    assert!(out.status.success(), "--list fails");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| {
            !line.starts_with("together ")
                && !line.starts_with("heavy ")
                && !line.starts_with("cli_only ")
        })
        .map(|line| {
            let mut words = line.split_whitespace();
            let half = words.next().expect("a half").to_owned();
            let lane = words.next().expect("a lane").to_owned();
            let name = words.next().expect("a name").to_owned();
            let reads = words.next().expect("its reads").to_owned();
            let command = words.collect::<Vec<_>>().join(" ");
            Step {
                half,
                lane,
                name,
                reads,
                command,
            }
        })
        .collect()
}

/// The groups `--list` declares: steps kept or run together.
fn togethers() -> Vec<Vec<String>> {
    let out = Command::new(repo().join("scripts").join("merge-gate.sh"))
        .arg("--list")
        .output()
        .expect("merge-gate.sh runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("together "))
        .map(|names| names.split_whitespace().map(str::to_owned).collect())
        .collect()
}

/// The steps `--list` declares heavy, in the order they take the machine: each waits until no
/// other heavy step runs.
fn heavy_order() -> Vec<String> {
    let out = Command::new(repo().join("scripts").join("merge-gate.sh"))
        .arg("--list")
        .output()
        .expect("merge-gate.sh runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("heavy "))
        .flat_map(str::split_whitespace)
        .map(str::to_owned)
        .collect()
}

/// The heavy steps as a set.
fn heavies() -> BTreeSet<String> {
    heavy_order().into_iter().collect()
}

/// A test source finds the owner's data tree through the instance file's variable, the one
/// helper over it (`tests/this_box.rs`), or the web tests' live-chain variables that
/// `scripts/web-test.sh` sets from it. A file with no test of its own is a helper, read where
/// its callers are.
fn reads_the_data_tree(source: &str) -> bool {
    (source.contains("#[test]") || source.contains("test("))
        && (source.contains("instance::ENV")
            || source.contains("this_box::")
            || source.contains("SUZATARY_LIVE_"))
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// `all.rs`'s modules, as (file, module): `#[path = "<file>"]` over `mod <module>;`.
fn all_modules() -> Vec<(String, String)> {
    let all = std::fs::read_to_string(repo().join("app/cli/tests/all.rs")).expect("all.rs reads");
    let lines: Vec<&str> = all.lines().collect();
    lines
        .windows(2)
        .filter_map(|pair| {
            let file = pair[0].strip_prefix("#[path = \"")?.strip_suffix("\"]")?;
            let module = pair[1].strip_prefix("mod ")?.strip_suffix(';')?;
            Some((file.to_owned(), module.to_owned()))
        })
        .collect()
}

#[test]
fn merge_gate_has_two_halves_and_every_step_is_in_one() {
    let steps = steps();
    assert!(!steps.is_empty());
    for step in &steps {
        assert!(
            step.half == "tree" || step.half == "live",
            "{}'s half is {:?}",
            step.name,
            step.half
        );
    }
    assert!(steps.iter().any(|s| s.half == "tree") && steps.iter().any(|s| s.half == "live"));
}

#[test]
fn merge_gate_every_test_that_reads_the_data_tree_is_in_the_live_half() {
    let live: Vec<String> = steps()
        .into_iter()
        .filter(|s| s.half == "live")
        .map(|s| s.command)
        .collect();
    let words = |c: &String| c.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    let runs_target = |target: &str| {
        live.iter().any(|c| {
            words(c)
                .windows(2)
                .any(|w| w[0] == "--test" && w[1] == target)
        })
    };
    let filters_module = |module: &str| {
        live.iter().any(|c| {
            c.split_once(" -- ").is_some_and(|(before, filters)| {
                before.contains("--test all")
                    && filters
                        .split_whitespace()
                        .any(|f| f == format!("{module}::"))
            })
        })
    };
    let modules = all_modules();
    assert!(!modules.is_empty(), "all.rs lists its modules");
    let mut missing = Vec::new();

    let mut rust = Vec::new();
    for krate in std::fs::read_dir(repo().join("app"))
        .expect("app/")
        .flatten()
    {
        files_under(&krate.path().join("tests"), &mut rust);
    }
    for file in rust
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "rs"))
    {
        let source = std::fs::read_to_string(file).unwrap_or_default();
        if !reads_the_data_tree(&source) {
            continue;
        }
        let tests = file
            .ancestors()
            .find(|a| a.file_name().is_some_and(|n| n == "tests"))
            .expect("under a tests/");
        let relative = file.strip_prefix(tests).expect("below it");
        // A file is its target's (`tests/<target>.rs` and `tests/<target>/…`), or a module of
        // the cli's `all` target, which the live half filters by module.
        let target = relative
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .map(|c| c.trim_end_matches(".rs").to_owned())
            .unwrap_or_default();
        let module = modules
            .iter()
            .find(|(f, _)| Path::new(f) == relative)
            .map(|(_, m)| m);
        let covered = match module {
            Some(m) if tests.ends_with("cli/tests") => filters_module(m),
            _ => runs_target(&target),
        };
        if !covered {
            missing.push(file.display().to_string());
        }
    }

    let mut web = Vec::new();
    files_under(&repo().join("web/test"), &mut web);
    for file in web {
        let source = std::fs::read_to_string(&file).unwrap_or_default();
        if reads_the_data_tree(&source)
            && !live
                .iter()
                .any(|c| c.split_whitespace().any(|w| w == "scripts/web-test.sh"))
        {
            missing.push(file.display().to_string());
        }
    }

    assert!(
        missing.is_empty(),
        "these tests read the owner's data tree, so the tree half (where the data is hidden) \
         skips them, and no step of the live half runs them: add them to scripts/merge-gate.sh's \
         live half: {missing:?}"
    );
}

/// A filter that names no module runs nothing and passes, so each module the live half names
/// is one `all.rs` has.
#[test]
fn merge_gate_live_modules_are_modules_of_all() {
    let modules = all_modules();
    let live_all = steps()
        .into_iter()
        .find(|s| s.half == "live" && s.command.contains("--test all"))
        .expect("the live half runs the all target's live modules");
    let named: Vec<&str> = live_all
        .command
        .split_once(" -- ")
        .expect("filters after --")
        .1
        .split_whitespace()
        .collect();
    assert!(!named.is_empty());
    for filter in named {
        let module = filter.strip_suffix("::").unwrap_or(filter);
        assert!(
            modules.iter().any(|(_, m)| m == module),
            "the live half filters on {filter}, which is no module of app/cli/tests/all.rs"
        );
    }
}

/// The triage push runs in the live half over the instance's data repository, and only ever
/// dry, so the gate sees what the box's next push would stage and commits nothing there.
#[test]
fn merge_gate_live_half_checks_the_public_tree_whole() {
    let step = steps()
        .into_iter()
        .find(|s| s.name == "public-check")
        .expect("the gate checks HEAD's public tree");
    assert_eq!(step.half, "live", "it reads the data repository's lists");
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    assert!(
        gate.contains("scripts/public-tree.sh --check --strict"),
        "as a publish checks it, every rule run"
    );
}

/// The live half runs the triage push dry, once.
#[test]
fn merge_gate_live_half_runs_the_push_dry() {
    let step = steps()
        .into_iter()
        .find(|s| s.name == "push-dry")
        .expect("the gate has a push-dry step");
    assert_eq!(step.half, "live");
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let pushes: Vec<&str> = gate
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && line.contains("safe-push.sh"))
        .collect();
    assert_eq!(
        pushes,
        vec!["      scripts/safe-push.sh --dry"],
        "the gate runs safe-push.sh once, dry"
    );
}

/// The live half reads the owner's data tree, which no name of the tree covers, so none of its
/// steps is kept but the live pins and modules, which are kept by the code that reads the data
/// (`merge_gate_live_reads_cover_what_the_live_tests_compile`), and the web's live run, kept by
/// the web's reads as the tree half's web steps are; and every step's reads is one of the forms.
#[test]
fn merge_gate_keeps_no_live_step() {
    for step in steps() {
        assert!(
            step.whole() == "all"
                || step.reads == "unkept"
                || !step.skips().is_empty()
                || step.only().is_some(),
            "{}'s reads is {:?}",
            step.name,
            step.reads
        );
        if step.half == "live" && !["live", "live-all", "web-live"].contains(&step.name.as_str()) {
            assert_eq!(step.reads, "unkept", "the live step {} is kept", step.name);
        }
    }
}

/// Whether a Rust source's code (its comments aside) names `path` as one: `path/…`, the path
/// as a whole literal or a literal's last segments (`"../../web/src"`, a `concat!`'s
/// `"/../../web/typst"`), a single name inside a `join` or a `Path::new` (a bare word is also
/// data), or its segments joined one by one.
fn names_path(source: &str, path: &str) -> bool {
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let source = code.as_str();
    let joined = path
        .split('/')
        .map(|segment| format!(".join(\"{segment}\")"))
        .collect::<Vec<_>>()
        .concat();
    source.contains(&format!("{path}/"))
        || (path.contains('/') && source.contains(&format!("\"{path}\"")))
        || source.contains(&format!("/{path}\""))
        || source.contains(&format!("join(\"{path}\")"))
        || source.contains(&format!("new(\"{path}\")"))
        || source.contains(&joined)
}

/// The sources that name a skipped path only as a path of a scratch repository, of a checkout
/// handed over at run time, of a branch read through git, of a literal under test or of a
/// document a printed line points a person to, never to read this repository's own: each one
/// read by a person, so a new source naming one fails until it is read too.
const NAMED_AS_DATA: &[(&str, &str)] = &[
    ("app/cli/src/commands/doctor.rs", "docs"),
    ("app/cli/src/commands/fixture.rs", "docs"),
    ("app/cli/src/commands/init/onboarding.rs", "docs"),
    ("app/cli/src/commands/release.rs", "CHANGELOG.md"),
    ("app/cli/tests/coding.rs", "docs"),
    ("app/cli/tests/mcp.rs", "docs"),
    ("app/cli/tests/release.rs", "CHANGELOG.md"),
    ("app/core/src/coding.rs", "docs"),
    ("app/core/src/leak.rs", "docs"),
    ("app/core/src/repos.rs", "docs"),
    ("app/core/tests/act_literal.rs", "docs"),
    ("app/core/tests/web_vectors.rs", "docs"),
    ("app/io/src/repos.rs", "docs"),
];

/// The paths only a declaring unit reads, as `--list` prints them.
fn cli_only() -> Vec<String> {
    let out = Command::new(repo().join("scripts").join("merge-gate.sh"))
        .arg("--list")
        .output()
        .expect("merge-gate.sh runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("cli_only "))
        .expect("the gate lists its cli_only paths")
        .split(',')
        .map(str::to_owned)
        .collect()
}

/// The workspace's members, each with its directory under the repository, from cargo metadata.
fn members() -> Vec<(String, String)> {
    let out = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
            "--manifest-path",
        ])
        .arg(repo().join("app/Cargo.toml"))
        .output()
        .expect("cargo metadata runs");
    assert!(out.status.success(), "cargo metadata: {out:?}");
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).expect("it parses");
    metadata["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|p| {
            let dir = Path::new(p["manifest_path"].as_str().expect("a manifest"))
                .parent()
                .expect("a directory")
                .strip_prefix(repo())
                .expect("under the repository")
                .to_string_lossy()
                .into_owned();
            (p["name"].as_str().expect("a name").to_owned(), dir)
        })
        .collect()
}

/// Whether `member`'s unit declares a read of `path` in `crate_reads`.
fn declares(member: &str, path: &str) -> bool {
    crate_reads().iter().any(|entry| {
        entry.split_once('=').is_some_and(|(name, read)| {
            name == member && (path == read || path.starts_with(&format!("{read}/")))
        })
    })
}

/// A cargo step that declares it never reads a path is kept across changes to it, so no Rust
/// source it builds or runs may name that path; a path only a declaring unit reads may be named
/// by the sources of a member whose unit declares it. The guards' own sources are read by the
/// `guards` step, which reads the whole tree.
#[test]
fn merge_gate_no_rust_source_names_a_path_its_cargo_steps_skip() {
    let skipped: BTreeSet<String> = steps()
        .iter()
        .filter(|s| s.command.contains("cargo"))
        .flat_map(|s| s.skips().into_iter().map(str::to_owned).collect::<Vec<_>>())
        .collect();
    assert!(!skipped.is_empty(), "the cargo steps skip the web");
    let guards = repo().join("app/cli/tests/guards");
    let mut sources = Vec::new();
    for krate in std::fs::read_dir(repo().join("app"))
        .expect("app/")
        .flatten()
    {
        if !krate.file_name().to_string_lossy().starts_with("target") {
            files_under(&krate.path(), &mut sources);
        }
    }
    let (cli_only, members) = (cli_only(), members());
    let mut named = Vec::new();
    for file in sources.iter().filter(|f| {
        f.extension().is_some_and(|e| e == "rs")
            && !f.starts_with(&guards)
            && !f.ends_with("cli/tests/guards.rs")
    }) {
        let source = std::fs::read_to_string(file).unwrap_or_default();
        let relative = file.strip_prefix(repo()).unwrap_or(file).to_string_lossy();
        let member = members
            .iter()
            .filter(|(_, dir)| relative.starts_with(&format!("{dir}/")))
            .max_by_key(|(_, dir)| dir.len())
            .map(|(name, _)| name.as_str());
        for path in &skipped {
            if NAMED_AS_DATA.contains(&(relative.as_ref(), path.as_str()))
                || (cli_only.contains(path) && member.is_some_and(|m| declares(m, path)))
            {
                continue;
            }
            if names_path(&source, path) {
                named.push(format!("{} names {path}", file.display()));
            }
        }
    }
    assert!(
        named.is_empty(),
        "a cargo step of scripts/merge-gate.sh declares it never reads these paths, so its \
         pass is kept when they change: {named:?}"
    );
}

/// The stub run in every step's place: it logs the step's name and fails a step named in
/// `red`.
const STUB: &str = r#"#!/bin/sh
echo "$1" >>"$GATE_PROBES"
echo "$1 ${SUZATARY_GATE_UNITS:-*}" >>"$GATE_PROBES.units"
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
        for path in [
            "app/core/src/lib.rs",
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
        gate.write(".gitignore", "red\nprobes*\nstub*\n");
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !started.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the first gate never ran a step"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

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

/// The public-tree step leaves out of its key the private history it never reads, read off
/// `.gitattributes`: what it declares `export-ignore`, which `git archive` leaves out, but the
/// changelog the public one is dated from and the attributes themselves, which `git archive`
/// reads to decide; and docs/ and android/, which none of its builds' sources holds
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
        .chain(["docs".to_owned(), "android".to_owned()])
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

/// Each step that reads what another step makes outside the tree, as (maker, reader): e2e serves
/// the bundle build-web makes, the web steps read what web-inputs prepares, the plugin tests load
/// the components fence-plugin resolves, and deployed-shape runs the binary public-tree builds.
pub(super) const MADE_AND_READ: &[(&str, &str)] = &[
    ("build-web", "e2e"),
    ("web-inputs", "web-test"),
    ("web-inputs", "build-web"),
    ("fence-plugin", "plugin-fence"),
    ("fence-plugin", "plugin-sync"),
    ("public-tree", "deployed-shape"),
];

/// A step that reads another's output runs after it in its lane, or in the `quiet` lane, which
/// runs after every other, since lanes run side by side: e2e serves the bundle build-web makes,
/// the web steps read what web-inputs prepares, and the plugin tests load the
/// components fence-plugin resolves.
#[test]
fn merge_gate_a_step_follows_what_it_reads_in_its_lane() {
    let steps = steps();
    let at = |name: &str| {
        steps
            .iter()
            .position(|s| s.name == name)
            .unwrap_or_else(|| panic!("no step {name}"))
    };
    for &(made, reader) in MADE_AND_READ {
        let (m, r) = (&steps[at(made)], &steps[at(reader)]);
        assert!(
            m.half == r.half
                && ((m.lane == r.lane && at(made) < at(reader))
                    || (r.lane == "quiet" && m.lane != "quiet")),
            "{reader} reads what {made} makes, so it runs after it in its lane"
        );
    }
}

/// The workspace's Cargo builds and the Nix builds of the app and the browser module are heavy:
/// on the box two of them side by side ran it out of memory (train 90). Every name `heavy`
/// declares is a step, and the heavy steps of a lane are named in the lane's own order, since
/// each waits for the one named before it: a lane whose later step came first would wait on
/// itself.
#[test]
fn merge_gate_declares_the_builds_heavy() {
    let all = steps();
    let steps: BTreeSet<String> = all.iter().map(|s| s.name.clone()).collect();
    let order = heavy_order();
    for step in &all {
        let in_lane: Vec<&String> = all
            .iter()
            .filter(|s| s.half == step.half && s.lane == step.lane)
            .map(|s| &s.name)
            .filter(|n| order.contains(n))
            .collect();
        let as_named: Vec<&String> = order.iter().filter(|n| in_lane.contains(n)).collect();
        assert_eq!(in_lane, as_named, "the {} lane's heavy steps", step.lane);
    }
    let heavy = heavies();
    assert_eq!(heavy.len(), order.len(), "each heavy step is named once");
    let unknown: Vec<&String> = heavy.iter().filter(|h| !steps.contains(*h)).collect();
    assert!(unknown.is_empty(), "heavy names no step {unknown:?}");
    for build in [
        "clippy",
        "clippy-fixture",
        "doc",
        "build",
        "web-inputs",
        "public-tree",
        "deploy-build",
    ] {
        assert!(heavy.contains(build), "{build} is heavy");
    }
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
    let wait_for = |what: &str, done: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "never: {what}");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    };
    let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();

    let first_probes = gate.scratch.join("probes-first");
    let mut first = spawn(&gate.scratch, &first_probes, std::process::Stdio::null());
    let holding = tree_heavy_order().remove(0);
    wait_for("the first gate's heavy step runs", &|| {
        read(&first_probes).lines().any(|l| l == holding)
    });

    let second_probes = gate.scratch.join("probes-second");
    let second_err = gate.scratch.join("probes-second.err");
    let mut second = spawn(
        &tree,
        &second_probes,
        std::fs::File::create(&second_err).expect("its log").into(),
    );
    wait_for("the second gate's heavy step waits", &|| {
        read(&second_err).contains(&format!("waits for the heavy step {holding}"))
    });
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

/// A step that reads what another writes outside the tree is kept or run with it, so it never
/// reads what a kept writer left from another tree: e2e serves the bundle build-web writes to
/// web/dist, and web-test and build-web read the web/src/wire/generated web-inputs writes.
#[test]
fn merge_gate_keeps_a_writer_and_its_readers_together() {
    let groups = togethers();
    for (writer, reader) in [
        ("build-web", "e2e"),
        ("web-inputs", "web-test"),
        ("web-inputs", "build-web"),
    ] {
        assert!(
            groups
                .iter()
                .any(|g| g.iter().any(|n| n == writer) && g.iter().any(|n| n == reader)),
            "{reader} reads what {writer} writes outside the tree, so they are together: {groups:?}"
        );
    }
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

/// A kept scene reads nothing, so a scene is kept across a change that build-web runs for only
/// when its key holds every input of the bundle build-web writes: no path an e2e unit skips is
/// one the bundle is built from.
#[test]
fn merge_gate_a_scene_reads_every_input_of_the_bundle() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let base = gate
        .lines()
        .find_map(|l| l.trim().strip_prefix("[files:web/e2e]=\""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("the scenes' skips are declared");
    for skipped in base.split_whitespace() {
        for input in [
            "web/src",
            "web/static",
            "web/typst",
            "tokens.typ",
            "scripts",
            "app/core",
            "app/wasm",
        ] {
            assert!(
                !(input == skipped || input.starts_with(&format!("{skipped}/"))),
                "every scene skips {skipped}, which the bundle reads ({input})"
            );
        }
    }
    let shared = gate
        .lines()
        .find(|l| l.starts_with("declare -A unit_shared="))
        .expect("the shared paths are declared");
    assert!(
        shared.contains("[web/e2e]=\"web/src "),
        "every scene reads every module of the bundle: {shared}"
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
    gate.write("app/core/proptest-regressions/sync.txt", "cc 0123\n");
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

/// The 40-hex commit ids on the lines of `text` that name the Prodrome's repository.
fn prodrome_pins(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter(|line| line.contains("fulfilium/prodrome"))
        .flat_map(|line| line.split(|c: char| !c.is_ascii_hexdigit()))
        .filter(|word| word.len() == 40)
        .map(str::to_owned)
        .collect()
}

/// The tree half's web steps read the Prodrome twice: its crates through cargo (the wasm, the
/// wire and the vectors' parser, at `app/Cargo.lock`'s revision) and its conformance files
/// through the flake input (`suzatary-vector-json` reads `${prodrome}/conformance`, and the
/// tab's Typst is that input's build). One revision, pinned in all four files, so a vector is
/// always read by the parser of the commit that wrote it.
#[test]
fn merge_gate_reads_one_prodrome_through_cargo_and_the_flake() {
    let read = |path: &str| {
        std::fs::read_to_string(repo().join(path)).unwrap_or_else(|e| panic!("{path} reads: {e}"))
    };
    let lock: serde_json::Value =
        serde_json::from_str(&read("flake.lock")).expect("flake.lock parses");
    let locked = lock["nodes"]["prodrome"]["locked"]["rev"]
        .as_str()
        .expect("flake.lock locks the prodrome input at a revision")
        .to_owned();
    for path in ["flake.nix", "app/Cargo.toml", "app/Cargo.lock"] {
        assert_eq!(
            prodrome_pins(&read(path)),
            BTreeSet::from([locked.clone()]),
            "{path} pins the Prodrome where flake.lock does"
        );
    }
}

/// The deploy's build is made once, by the gate: the live half builds HEAD's public tree on the
/// box with the list `scripts/deploy.sh` builds (`scripts/app-builds.sh`), from the store path a
/// published commit is fetched to, so the deploy of a landed commit finds every output built.
/// And the dry push and the leak check run the binary that build made, the program the deploy runs:
/// this gate's, handed on in its lane's shell, never a link another checkout's gate moves, and
/// none at all after a failed build.
#[test]
fn merge_gate_live_half_builds_what_the_deploy_builds() {
    let step = steps()
        .into_iter()
        .find(|s| s.name == "deploy-build")
        .expect("the gate builds the deploy's outputs");
    assert_eq!(step.half, "live", "on the box, for its own system");
    assert_eq!(step.command, "public_tree", "from HEAD's public tree");
    let read = |path: &str| std::fs::read_to_string(repo().join(path)).expect("reads");
    let (gate, deploy) = (read("scripts/merge-gate.sh"), read("scripts/deploy.sh"));
    assert!(
        gate.contains("tree=\"$(scripts/public-tree.sh --store HEAD)\"")
            && gate.contains("scripts/app-builds.sh \"$tree\""),
        "the public tree's own store path, built with app-builds.sh"
    );
    assert!(
        deploy.contains("\"$here/scripts/app-builds.sh\" \"$app\""),
        "the list the deploy builds"
    );
    let all = steps();
    let at = |name: &str| {
        all.iter()
            .position(|s| s.name == name)
            .unwrap_or_else(|| panic!("no step {name}"))
    };
    for reader in ["push-dry", "public-check"] {
        assert!(
            all[at(reader)].lane == step.lane && at(reader) > at("deploy-build"),
            "{reader} runs the deploy's binary, after its build, in its lane"
        );
    }
    assert!(
        gate.contains(
            "SUZATARY_DEPLOY_BIN=\"$(nix build \"$tree#suzatary\" --no-link --print-out-paths)/bin/suzatary\""
        ) && gate.contains("unset SUZATARY_DEPLOY_BIN"),
        "the build names the binary it built, and nothing before it succeeds"
    );
    assert_eq!(
        gate.matches("\"${SUZATARY_DEPLOY_BIN:?deploy-build made no binary}\"")
            .count(),
        2,
        "push-dry and public-check run that binary, or fail"
    );
    for reader in ["push_dry() (", "public_check() ("] {
        assert!(
            gate.lines().any(|l| l == reader),
            "{reader} runs in a subshell, so a missing binary fails the step, not the lane"
        );
    }
    assert!(
        gate.lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .all(|l| !l.contains("public-bin")),
        "no step reads the link every checkout's gate moves"
    );
}

/// The gate roots the environment it runs in before anything collects: `nix develop` roots
/// nothing, and a collection took gitleaks from under the guards mid-gate on 2026-10-11. So the
/// rooting comes before the room is made, and what it roots holds every store path on the
/// running shell's PATH in its closure, which is what a collection keeps.
#[test]
fn merge_gate_roots_the_shell_it_runs_in_before_it_collects() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let line = |needle: &str| {
        gate.lines()
            .position(|l| l.trim_start().starts_with(needle))
            .unwrap_or_else(|| panic!("the gate has no line starting {needle:?}"))
    };
    let call = gate
        .lines()
        .position(|l| l.trim() == "codespace_make_room")
        .expect("the gate makes room");
    assert!(
        line("if ! root_environment") < call,
        "the environment is rooted before the room is made"
    );
    let collects: Vec<usize> = gate
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("nix-collect-garbage") && !l.trim_start().starts_with('#'))
        .map(|(i, _)| i)
        .collect();
    let room = line("codespace_make_room() {");
    let room_end = room
        + gate
            .lines()
            .skip(room)
            .position(|l| l == "}")
            .expect("its end");
    assert!(
        collects.iter().all(|&i| room < i && i < room_end),
        "the room is the gate's one collector"
    );

    let on_path: Vec<String> = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .filter_map(|dir| dir.strip_prefix("/nix/store/"))
        .filter_map(|rest| rest.split('/').next())
        .map(|name| format!("/nix/store/{name}"))
        .collect();
    if on_path.is_empty() {
        eprintln!("merge_gate_roots_the_shell_it_runs_in_before_it_collects: not in a Nix shell");
        return;
    }
    let scratch = Scratch::new("root-environment");
    let roots = scratch.join("environment");
    let out = Command::new("bash")
        .arg("-c")
        .arg(r#"root="$1" && . "$1/scripts/nix-out.sh" && root_environment "$2" && nix-store -qR "$2"/path*"#)
        .arg("bash")
        .arg(repo())
        .arg(&roots)
        .output()
        .expect("bash runs");
    assert!(out.status.success(), "root_environment: {out:?}");
    let closure: BTreeSet<&str> = std::str::from_utf8(&out.stdout)
        .expect("paths")
        .lines()
        .collect();
    let unheld: Vec<&String> = on_path
        .iter()
        .filter(|p| !closure.contains(p.as_str()))
        .collect();
    assert!(
        unheld.is_empty(),
        "a collection would take these tools of the running shell: {unheld:?}"
    );
}

/// A Codespace's gate making room (`codespace_make_room`, run from the script's own text) with
/// `free_gb` GB free, stubs for the disk, cargo, the compiler and the store, in a scratch
/// checkout whose target holds one marker, after `edit` runs on it. Whether the target was kept,
/// and the stubs' log: each collection and `cargo sweep`, in order.
fn make_room(scratch: &Path, free_gb: u32, edit: &str) -> (bool, Vec<String>) {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let start = gate.find("\ncodespace_room=").expect("the room's settings");
    let end = start
        + gate[start..]
            .find("\n}\n")
            .and_then(|f| gate[start + f + 3..].find("\n}\n").map(|g| f + 3 + g + 3))
            .expect("codespace_make_room's end");
    let bin = scratch.join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    for (name, body) in [
        (
            "cargo",
            r#"if [ "$1" = sweep ]; then echo "$*" >>log; echo $(($(cat free) + 2)) >free; else echo cargo 1; fi"#,
        ),
        ("rustc", "echo rustc 1"),
        ("nix-collect-garbage", "echo collect >>log"),
        ("sudo", "true"),
    ] {
        let _ =
            suzatary_testkit::fake_program(&bin, name, &format!("#!/usr/bin/env bash\n{body}\n"));
    }
    let free = format!("{free_gb}\n");
    for (path, text) in [
        ("app/Cargo.lock", "lock\n"),
        ("flake.lock", "{}\n"),
        (
            "app/Cargo.toml",
            "[profile.dev]\n# why\nopt-level = 0\n\n[profile.release]\nlto = true\n",
        ),
        ("app/target/debug/marker", ""),
        ("free", free.as_str()),
        ("log", ""),
    ] {
        let path = scratch.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(path, text).expect("writes");
    }
    let script = format!(
        "{}\nfree_gb() {{ cat free; }}\n\
         codespace_toolchain_now >\"$codespace_toolchain\" && codespace_locks_now >\"$codespace_locks\" &&\n\
         {edit}\ncodespace_make_room",
        &gate[start..end]
    );
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .current_dir(scratch)
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env_remove("RUSTFLAGS")
        .output()
        .expect("bash runs");
    assert!(out.status.success(), "codespace_make_room: {out:?}");
    let log = std::fs::read_to_string(scratch.join("log")).expect("log");
    (
        scratch.join("app/target/debug/marker").exists(),
        log.lines().map(str::to_owned).collect(),
    )
}

/// A Codespace's room is swept by age alone, oldest first, and only until there is room: never
/// `cargo sweep --maxsize`, whose first fit (0.8.0) deletes the newest set's small units once one
/// older unit does not fit, and took the set the gate before had just built (the gate-build-once
/// report). And a move of a dev or test profile, or of the Rust flags, clears the target as a new
/// compiler does, where a release profile or a comment moves nothing.
#[test]
fn merge_gate_sweeps_a_codespace_target_by_age_alone() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    assert!(
        gate.lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .all(|l| !l.contains("--maxsize")),
        "no sweep by size"
    );
    let scratch = Scratch::new("make-room");
    let sweeps = |from: u32| {
        let (kept, log) = make_room(&scratch, from, "true");
        assert!(kept, "a warm target is kept");
        log
    };
    assert_eq!(
        sweeps(4),
        [
            "collect",
            "sweep --time 7 app",
            "sweep --time 3 app",
            "sweep --time 1 app"
        ],
        "the store first, then the units a week unread, three days, one"
    );
    assert_eq!(
        sweeps(6),
        ["collect", "sweep --time 7 app", "sweep --time 3 app"],
        "and no younger sweep once there is room"
    );
    assert_eq!(sweeps(20), Vec::<String>::new(), "nothing with room");

    for (edit, clears) in [
        (
            "sed -i 's/opt-level = 0/opt-level = 1/' app/Cargo.toml",
            true,
        ),
        ("export RUSTFLAGS=-Cforce-frame-pointers", true),
        ("sed -i 's/lto = true/lto = false/' app/Cargo.toml", false),
        ("sed -i 's/# why/# because/' app/Cargo.toml", false),
    ] {
        let (kept, _) = make_room(&scratch, 20, edit);
        assert_eq!(!kept, clears, "{edit}");
    }
}

/// Whether `tool` is on PATH.
fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()))
}

/// A throwaway workspace of four members (`suzatary-core`, `suzatary-io` on it, `suzatary` on
/// that, `suzatary-bench` on the core) and test files over two modules, written into `gate`.
fn units_fixture(gate: &Gate) {
    gate.write(
        "app/Cargo.toml",
        "[workspace]\nresolver = \"3\"\nmembers = [\"core\", \"io\", \"cli\", \"bench\"]\n",
    );
    for (dir, name, dep) in [
        ("core", "suzatary-core", ""),
        (
            "io",
            "suzatary-io",
            "suzatary-core = { path = \"../core\" }\n",
        ),
        ("cli", "suzatary", "suzatary-io = { path = \"../io\" }\n"),
        (
            "bench",
            "suzatary-bench",
            "suzatary-core = { path = \"../core\" }\n",
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
    gate.write(
        "web/test/a.test.ts",
        "import { a } from \"../src/a\"\nconsole.log(a)\n",
    );
    gate.write(
        "web/test/b.test.ts",
        "import { b } from \"../src/b\"\nconsole.log(b)\n",
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
/// drives the whole bundle. `--full` runs every step whole and reads no pass.
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

    gate.write("app/io/src/lib.rs", "a changed line\n");
    gate.commit();
    assert!(gate.run(&[]).0);
    assert_eq!(
        gate.units_ran()["test"],
        set(&["suzatary-io", "suzatary"]),
        "the member and the member that depends on it, never the core or the bench"
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
        "app/bench/Cargo.toml",
        "[package]\nname = \"suzatary-bench\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nsuzatary-core = { path = \"../core\" }\n\n[features]\nmore = []\n",
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

/// The paths `<dir>` names that the merge gate declares a workspace member reads beyond its
/// dependencies (`crate_reads`), as `<package>=<path>`.
fn crate_reads() -> Vec<String> {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    gate.lines()
        .find_map(|l| l.strip_prefix("crate_reads=("))
        .and_then(|l| l.strip_suffix(')'))
        .expect("the gate declares crate_reads")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The paths every Rust unit skips unless it declares them: the fmt step's.
fn rust_skips() -> Vec<String> {
    steps()
        .into_iter()
        .find(|s| s.name == "fmt")
        .expect("the fmt step")
        .skips()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// The workspace members' units as the gate computes them: each member with the paths it does
/// not read.
fn member_units() -> Vec<(String, Vec<String>)> {
    let scratch = Scratch::new("member-units");
    let metadata = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
            "--manifest-path",
        ])
        .arg(repo().join("app/Cargo.toml"))
        .output()
        .expect("cargo metadata runs");
    assert!(metadata.status.success(), "cargo metadata: {metadata:?}");
    std::fs::write(scratch.join("metadata.json"), &metadata.stdout).expect("writes");
    let tree = Command::new("git")
        .args(["ls-tree", "-r", "--full-tree", "HEAD"])
        .current_dir(repo())
        .output()
        .expect("git runs");
    assert!(tree.status.success(), "git ls-tree: {tree:?}");
    std::fs::write(scratch.join("tree"), &tree.stdout).expect("writes");
    let out = Command::new("node")
        .arg(repo().join("scripts/gate-units.mjs"))
        .arg("crates")
        .arg(scratch.join("metadata.json"))
        .arg(repo())
        .arg(scratch.join("tree"))
        .arg("--base")
        .args(rust_skips())
        .arg("--")
        .args(crate_reads())
        .output()
        .expect("node runs");
    assert!(out.status.success(), "gate-units.mjs: {out:?}");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, skips)| {
            (
                name.to_owned(),
                skips.split_whitespace().map(str::to_owned).collect(),
            )
        })
        .collect()
}

/// `path` with its `.` and `..` segments folded, or nothing when it climbs out of the root.
fn folded(path: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    Some(parts.join("/"))
}

/// A member's tests are kept across a change to a path its unit skips, so every path its sources
/// reach by a relative literal is one the unit reads: an `include_*!` against the file's own
/// directory, and any other literal holding `../` against the member's directory, as
/// `CARGO_MANIFEST_DIR` and a test's working directory are. The guards' sources are the guards
/// step's, which reads the whole tree.
#[test]
fn merge_gate_member_sources_reach_only_what_their_units_read() {
    if !on_path("node") {
        eprintln!("merge_gate_member_sources_reach_only_what_their_units_read: no node");
        return;
    }
    let dirs: std::collections::BTreeMap<String, String> = members().into_iter().collect();
    let guards = repo().join("app/cli/tests/guards");
    let mut reached = Vec::new();
    for (member, skips) in member_units() {
        let dir = &dirs[&member];
        let mut sources = Vec::new();
        files_under(&repo().join(dir), &mut sources);
        for file in sources.iter().filter(|f| {
            f.extension().is_some_and(|e| e == "rs")
                && !f.starts_with(&guards)
                && !f.ends_with("cli/tests/guards.rs")
        }) {
            let source = std::fs::read_to_string(file).unwrap_or_default();
            let code: String = source
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            let parent = file
                .parent()
                .expect("a parent")
                .strip_prefix(repo())
                .expect("under the repository")
                .to_path_buf();
            let mut paths = Vec::new();
            for (at, _) in code.match_indices("include_") {
                let rest = &code[at..];
                if let Some((_, literal)) = rest.split_once("!(\"")
                    && let Some((literal, _)) = literal.split_once('"')
                    && rest.find("!(\"").is_some_and(|i| i < 16)
                {
                    paths.push(folded(&parent.join(literal)));
                }
            }
            for literal in code
                .split('"')
                .skip(1)
                .step_by(2)
                .filter(|l| l.contains("../"))
            {
                paths.push(folded(
                    &Path::new(dir).join(literal.trim_start_matches('/')),
                ));
            }
            for path in paths.into_iter().flatten() {
                if skips
                    .iter()
                    .any(|s| path == *s || path.starts_with(&format!("{s}/")))
                {
                    reached.push(format!("{} ({member}) reaches {path}", file.display()));
                }
            }
        }
    }
    assert!(
        reached.is_empty(),
        "these sources reach a path their unit skips (declare it in scripts/merge-gate.sh's \
         crate_reads): {reached:?}"
    );
}

/// The live pins and modules are kept by `live_reads`, so it holds everything they compile and
/// read of the tree: the workspace's manifests and lock, the directory of every member the CLI
/// depends on (the store, its readers, the wire and the migrations among them), the CLI's
/// sources, the test files and the modules they include, and every file the members' sources
/// include at compile time.
#[test]
#[allow(clippy::too_many_lines)]
fn merge_gate_live_reads_cover_what_the_live_tests_compile() {
    // The public tree holds no live pins, and its gate no `live` step.
    let live = steps()
        .into_iter()
        .find(|s| s.name == "live-all")
        .expect("the live modules' step");
    if let Some(pins) = steps().into_iter().find(|s| s.name == "live") {
        assert_eq!(pins.reads, live.reads, "one list");
    }
    let covered = |path: &str| live.reads_path(path);
    let mut missing = Vec::new();
    let mut need = |path: String| {
        if !covered(&path) {
            missing.push(path);
        }
    };
    need("app/Cargo.toml".to_owned());
    need("app/Cargo.lock".to_owned());
    let metadata = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
            "--manifest-path",
        ])
        .arg(repo().join("app/Cargo.toml"))
        .output()
        .expect("cargo metadata runs");
    assert!(metadata.status.success());
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).expect("metadata parses");
    let packages = metadata["packages"].as_array().expect("packages");
    let dir_of = |p: &serde_json::Value| {
        Path::new(p["manifest_path"].as_str().expect("a manifest"))
            .parent()
            .expect("a directory")
            .strip_prefix(repo())
            .expect("under the repository")
            .to_string_lossy()
            .into_owned()
    };
    let mut closure = vec!["suzatary".to_owned()];
    let mut i = 0;
    while i < closure.len() {
        let package = packages
            .iter()
            .find(|p| p["name"] == closure[i].as_str())
            .expect("a member");
        for dep in package["dependencies"].as_array().expect("deps") {
            let (Some(name), Some(_)) = (dep["name"].as_str(), dep["path"].as_str()) else {
                continue;
            };
            if !closure.iter().any(|c| c == name) {
                closure.push(name.to_owned());
            }
        }
        i += 1;
    }
    for package in packages {
        let dir = dir_of(package);
        need(format!("{dir}/Cargo.toml"));
        let name = package["name"].as_str().expect("a name");
        if name == "suzatary" {
            need(format!("{dir}/src"));
        } else if closure.iter().any(|c| c == name) {
            need(dir.clone());
        }
        if closure.iter().any(|c| c == name) {
            let mut sources = Vec::new();
            files_under(&repo().join(&dir).join("src"), &mut sources);
            for file in sources {
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                for include in text
                    .split("include_str!(\"")
                    .chain(text.split("include_bytes!(\""))
                    .skip(1)
                {
                    let Some((rel, _)) = include.split_once('"') else {
                        continue;
                    };
                    let target = file.parent().expect("a parent").join(rel);
                    if let Ok(target) = target.canonicalize()
                        && let Ok(rel) = target.strip_prefix(repo().canonicalize().expect("repo"))
                    {
                        need(rel.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    need("app/cli/tests/live.rs".to_owned());
    need("app/cli/tests/all.rs".to_owned());
    for target in [
        "live.rs",
        "accounts.rs",
        "context_headroom.rs",
        "live_blocks.rs",
        "perf_fold_chain.rs",
    ] {
        let text =
            std::fs::read_to_string(repo().join("app/cli/tests").join(target)).unwrap_or_default();
        need(format!("app/cli/tests/{target}"));
        for path in cli_only() {
            if text.contains(&format!("\"{path}\"")) {
                need(path);
            }
        }
        for line in text.lines() {
            if let Some(path) = line
                .trim()
                .strip_prefix("#[path = \"")
                .and_then(|l| l.strip_suffix("\"]"))
            {
                need(format!("app/cli/tests/{path}"));
            }
            if line.trim() == "mod common;" {
                need("app/cli/tests/common".to_owned());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "the live tests compile or read these, and scripts/merge-gate.sh's live_reads leaves \
         them out: {missing:?}"
    );
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let modules: Vec<&str> = gate
        .lines()
        .find_map(|l| l.strip_prefix("live_modules=("))
        .and_then(|l| l.strip_suffix(')'))
        .expect("live_modules")
        .split_whitespace()
        .collect();
    for module in modules {
        assert!(covered(&format!("app/cli/tests/{module}.rs")), "{module}");
    }
}

/// The web steps and the public tree's builds read no path they skip: the sources of the flake
/// builds they take (the browser module, the wire, the vectors, the bundle; the binary, the
/// plugins) hold no file under a path the steps declare they never read.
#[test]
fn merge_gate_web_and_public_builds_read_no_skipped_path() {
    if !on_path("nix") || std::env::var_os("IN_NIX_SHELL").is_none() {
        eprintln!("merge_gate_web_and_public_builds_read_no_skipped_path: not in a Nix shell");
        return;
    }
    let all = steps();
    let step = |name: &str| all.iter().find(|s| s.name == name).expect("a step");
    let checks = [
        (
            "web-inputs",
            ["suzatary-wasm", "suzatary-wire-ts", "suzatary-vector-json"].as_slice(),
        ),
        ("nix-web", ["suzatary-web"].as_slice()),
        (
            "public-tree",
            // The binary's sources: `.#suzatary` is the binary copied beside the release file,
            // a `runCommand` with no source of its own.
            ["suzatary-bin", "suzatary-web", "suzatary-plugins"].as_slice(),
        ),
    ];
    let mut read = Vec::new();
    for (name, outputs) in checks {
        let step = step(name);
        for output in outputs {
            let out = Command::new("nix")
                .args(["eval", "--raw"])
                .arg(format!("{}#{output}.src", repo().display()))
                .output()
                .expect("nix eval runs");
            assert!(out.status.success(), "nix eval {output}.src: {out:?}");
            let src = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            let mut files = Vec::new();
            files_under(&src, &mut files);
            assert!(!files.is_empty(), "{output}'s source holds files");
            for file in files {
                let rel = file.strip_prefix(&src).expect("under its source");
                let rel = rel.to_string_lossy();
                // The public tree's builds hold no private history, which its step skips too.
                let reads = if name == "public-tree" {
                    let private = step
                        .skips()
                        .into_iter()
                        .filter(|p| *p != "docs" && *p != "android");
                    !["docs", "android"]
                        .iter()
                        .any(|p| rel.starts_with(&format!("{p}/")))
                        || private.into_iter().any(|p| rel.starts_with(p))
                } else {
                    step.reads_path(&rel)
                };
                if !reads {
                    read.push(format!("{name} skips {rel}, which {output} is built from"));
                }
            }
        }
    }
    assert!(read.is_empty(), "{read:?}");
}

/// The browser module, the wire and the vectors are built from the core's library alone, so a
/// change to the CLI, the server's crates or a test file builds none of them again: their
/// sources hold, beside the workspace's manifests, the core's library, examples and release plan,
/// the browser module's crate, the calendar's schema, and of the core's tests only the supports
/// its library includes, each of which they hold.
#[test]
fn merge_gate_web_inputs_are_built_from_the_core_library_alone() {
    if !on_path("nix") || std::env::var_os("IN_NIX_SHELL").is_none() {
        eprintln!(
            "merge_gate_web_inputs_are_built_from_the_core_library_alone: not in a Nix shell"
        );
        return;
    }
    let library = ["app/core/src/", "app/core/examples/", "app/wasm/"];
    let files = [
        "app/Cargo.lock",
        "app/core/releases.toml",
        "app/plugin-ics/ics.schema",
    ];
    let mut sources = Vec::new();
    files_under(&repo().join("app/core/src"), &mut sources);
    let supports: BTreeSet<String> = sources
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| {
            text.lines()
                .filter_map(|l| l.trim().strip_prefix("#[path = \"../../tests/"))
                .filter_map(|l| l.strip_suffix("\"]"))
                .map(|rest| format!("app/core/tests/{rest}"))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        !supports.is_empty(),
        "the library includes its test supports"
    );
    for output in ["suzatary-wasm", "suzatary-wire-ts", "suzatary-vector-json"] {
        let out = Command::new("nix")
            .args(["eval", "--raw"])
            .arg(format!("{}#{output}.src", repo().display()))
            .output()
            .expect("nix eval runs");
        assert!(out.status.success(), "nix eval {output}.src: {out:?}");
        let src = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        let mut held = Vec::new();
        files_under(&src, &mut held);
        let held: BTreeSet<String> = held
            .iter()
            .map(|f| {
                f.strip_prefix(&src)
                    .expect("under its source")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let other: Vec<&String> = held
            .iter()
            .filter(|rel| {
                !(rel.ends_with("/Cargo.toml")
                    || files.contains(&rel.as_str())
                    || library.iter().any(|dir| rel.starts_with(dir))
                    || supports.contains(*rel))
            })
            .collect();
        assert!(other.is_empty(), "{output} is built from {other:?}");
        if output != "suzatary-wasm" {
            let missing: Vec<&String> = supports.iter().filter(|s| !held.contains(*s)).collect();
            assert!(
                missing.is_empty(),
                "{output}'s library test includes {missing:?}"
            );
        }
    }
}

/// No output holds the dependency builds the browser module and the core's builds start from,
/// so web-inputs roots them where it builds, on the box as on a Codespace: a collection that took
/// one cost a gate the wasm32 dependencies again.
#[test]
fn merge_gate_roots_the_web_inputs_dependency_builds() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let start = gate.find("\nweb_inputs() {").expect("the web inputs");
    let body = &gate[start..start + gate[start..].find("\n}").expect("its end")];
    for deps in ["suzatary-wasm-deps", "suzatary-core-deps"] {
        assert!(
            body.contains(&format!("nix_out {deps}")),
            "web-inputs roots {deps}"
        );
    }
}

/// The string literals of `source` that name a path of the repository from its root.
fn repo_paths(source: &str) -> Vec<String> {
    source
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| {
            ["app/", "web/", "docs/", "android/", "scripts/"]
                .iter()
                .any(|top| s.starts_with(top))
                || ["tokens.typ", "CHANGELOG.md", "CLAUDE.md"].contains(s)
        })
        .map(str::to_owned)
        .collect()
}

/// A unit test or a scene is kept by the files its imports reach and the paths every unit
/// reads, so no path it names from the repository's root (a file it reads at run time) is one
/// the units skip or one the graph alone decides.
#[test]
fn merge_gate_web_tests_name_only_paths_every_unit_reads() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let declared = |key: &str| -> Vec<String> {
        let line = gate
            .lines()
            .find_map(|l| l.trim().strip_prefix(&format!("[{key}]=\"")))
            .and_then(|l| l.split_once('"'))
            .map_or_else(|| panic!("{key} is declared"), |(v, _)| v.to_owned());
        line.split_whitespace().map(str::to_owned).collect()
    };
    let web = steps()
        .into_iter()
        .find(|s| s.name == "web-inputs")
        .expect("web-inputs")
        .skips()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let shared = |dir: &str| -> Vec<String> {
        let line = gate
            .lines()
            .find(|l| l.starts_with("declare -A unit_shared="))
            .expect("the shared paths are declared");
        line.split_once(&format!("[{dir}]=\""))
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(paths, _)| paths.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default()
    };
    // Paths a test holds as data (a report on a coding branch), never a file it reads.
    let data = ["docs/reports/x.md"];
    let mut wrong = Vec::new();
    let scopes = ["web/src", "web/test", "web/e2e"];
    for (dir, skips, shared) in [
        ("web/test", web, shared("web/test")),
        ("web/e2e", declared("files:web/e2e"), shared("web/e2e")),
    ] {
        let mut files = Vec::new();
        files_under(&repo().join(dir), &mut files);
        for file in files
            .iter()
            .filter(|f| f.extension().is_some_and(|e| e == "ts"))
        {
            let source = std::fs::read_to_string(file).unwrap_or_default();
            for path in repo_paths(&source)
                .into_iter()
                .filter(|p| !data.contains(&p.as_str()))
            {
                let under = |p: &str| path == p || path.starts_with(&format!("{p}/"));
                if skips.iter().any(|s| under(s))
                    || (scopes.iter().any(|s| under(s)) && !shared.iter().any(|s| under(s)))
                {
                    wrong.push(format!("{} names {path}", file.display()));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "these test files read a path their units are not keyed by (name it in \
         scripts/merge-gate.sh's unit_shared): {wrong:?}"
    );
}

/// Features unify across the workspace, so a member's manifest can change every member's test
/// executables (the core's `wasm` feature is turned on by `app/wasm/Cargo.toml`): no unit skips
/// any member's Cargo.toml, nor the workspace's manifest or lock.
#[test]
fn merge_gate_every_crate_unit_reads_every_manifest() {
    if !on_path("node") {
        eprintln!("merge_gate_every_crate_unit_reads_every_manifest: no node");
        return;
    }
    let mut manifests = vec!["app/Cargo.toml".to_owned(), "app/Cargo.lock".to_owned()];
    for krate in std::fs::read_dir(repo().join("app"))
        .expect("app/")
        .flatten()
    {
        if krate.path().join("Cargo.toml").is_file() {
            manifests.push(format!(
                "app/{}/Cargo.toml",
                krate.file_name().to_string_lossy()
            ));
        }
    }
    let units = member_units();
    assert!(units.len() > 1, "the workspace has members");
    for (member, skips) in units {
        for manifest in &manifests {
            assert!(
                !skips
                    .iter()
                    .any(|s| manifest == s || manifest.starts_with(&format!("{s}/"))),
                "{member}'s unit skips {manifest}"
            );
        }
    }
}

/// A release commit changes `app/release.toml` and `CHANGELOG.md` alone, so it reruns only what
/// reads them: the steps that read every file, and the one crate unit that declares the release
/// file (`suzatary`, whose guards hold the changelog's tests); no unit reads the changelog, and
/// the web's units skip both by their base.
#[test]
fn merge_gate_a_release_reruns_only_what_reads_it() {
    let release = ["app/release.toml", "CHANGELOG.md"];
    let mut rerun: Vec<String> = steps()
        .iter()
        .filter(|s| !s.reads.contains('~') && release.iter().any(|p| s.reads_path(p)))
        .map(|s| format!("{} {}", s.half, s.name))
        .collect();
    rerun.sort();
    assert_eq!(
        rerun,
        ["tree guards", "tree nix-module", "tree public-tree"],
        "the kept steps a release reruns"
    );
    let readers: Vec<String> = member_units()
        .into_iter()
        .flat_map(|(member, skips)| {
            release
                .iter()
                .filter(|p| {
                    !skips
                        .iter()
                        .any(|s| *p == s || p.starts_with(&format!("{s}/")))
                })
                .map(move |p| format!("{member} {p}"))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        readers,
        ["suzatary app/release.toml"],
        "the crate units a release reruns"
    );
}

/// The generated wire is outside the tree, so the units read what it imports as all of
/// web/src/wire: every import its generator writes resolves under web/src/wire.
#[test]
fn merge_gate_the_generated_wire_imports_only_the_wire() {
    let mut sources = Vec::new();
    files_under(&repo().join("app/core/src/wire"), &mut sources);
    sources.push(repo().join("app/core/src/wire.rs"));
    let mut imports = Vec::new();
    for file in sources
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "rs"))
    {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        for line in text
            .lines()
            .filter(|l| l.contains("import ") && l.contains(" from \\\""))
        {
            let Some(path) = line
                .split(" from \\\"")
                .nth(1)
                .and_then(|rest| rest.split_once("\\\""))
                .map(|(path, _)| path.to_owned())
            else {
                continue;
            };
            imports.push(path);
        }
    }
    assert!(!imports.is_empty(), "the generator writes imports");
    for path in imports {
        let resolved = Path::new("web/src/wire/generated").join(&path);
        let mut parts: Vec<String> = Vec::new();
        for part in resolved.components() {
            match part {
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
            }
        }
        let joined = parts.join("/");
        assert!(
            joined.starts_with("web/src/wire/"),
            "the generated wire imports {path}, outside web/src/wire"
        );
    }
}

/// The changelog step reads a train's entries on the commit before its release: the release
/// closes `## [Unreleased]`, so on the release itself the section reads as never moved.
#[test]
fn the_changelog_step_reads_the_commit_before_a_release() {
    let scratch = Scratch::new("gate-changelog-release");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&*scratch)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let write = |path: &str, text: &str| {
        let path = scratch.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(path, text).expect("writes");
    };
    let step = || {
        Command::new("bash")
            .arg("-c")
            .arg(format!(
                "set -euo pipefail; source <(sed -n '/^changelog_entry()/,/^}}/p' {}); changelog_entry",
                repo().join("scripts/merge-gate.sh").display()
            ))
            .current_dir(&*scratch)
            .output()
            .expect("bash runs")
            .status
            .success()
    };
    git(&["init", "-q", "-b", "master"]);
    write(
        "CHANGELOG.md",
        "# C\n\n## [Unreleased]\n\n## [1.0.0] - 2026-10-01\n\n- old\n",
    );
    write("app/x", "1\n");
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    git(&["update-ref", "refs/remotes/origin/master", "HEAD"]);

    write("app/x", "2\n");
    write(
        "CHANGELOG.md",
        "# C\n\n## [Unreleased]\n\n- new\n\n## [1.0.0] - 2026-10-01\n\n- old\n",
    );
    git(&["commit", "-q", "-am", "Merge #1: a change"]);
    assert!(step(), "an entry rides the change");

    write(
        "CHANGELOG.md",
        "# C\n\n## [Unreleased]\n\n## [1.1.0] - 2026-10-14\n\n- new\n\n## [1.0.0] - 2026-10-01\n\n- old\n",
    );
    git(&[
        "commit",
        "-q",
        "-am",
        "release: 1.1.0",
        "-m",
        "Release: 1.1.0",
    ]);
    assert!(step(), "the release is read on the commit before it");

    git(&["reset", "-q", "--hard", "origin/master"]);
    write("app/x", "3\n");
    git(&["commit", "-q", "-am", "Merge #2: no entry"]);
    write(
        "CHANGELOG.md",
        "# C\n\n## [Unreleased]\n\n## [1.1.0] - 2026-10-14\n\n## [1.0.0] - 2026-10-01\n\n- old\n",
    );
    git(&[
        "commit",
        "-q",
        "-am",
        "release: 1.1.0",
        "-m",
        "Release: 1.1.0",
    ]);
    assert!(
        !step(),
        "a train with no entry fails even under its release"
    );
}
