//! The merge gate's two halves (`scripts/merge-gate.sh`): every test that reads the owner's
//! data tree is in the live half, so the tree half, which runs where the data never is (GitHub's
//! runners) or with it hidden, leaves nothing unread that only the live half would read.
//! And its kept passes: a step whose inputs did not move is not run again, so each step's
//! declared reads must cover what it reads, and the gate run with a stub in every step's place
//! runs exactly the steps whose inputs moved.

use super::gate_list::{Step, heavies, heavy_order, steps, togethers};
use super::scratch::on_path;
use super::{Scratch, binary, repo};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    // The live half's web step runs the unit tests `live_web_units` names against the live chain.
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let units: Vec<&str> = gate
        .lines()
        .find_map(|l| l.strip_prefix("live_web_units=(")?.strip_suffix(')'))
        .expect("the live half names its web units")
        .split_whitespace()
        .collect();
    let web_live = live.iter().any(|c| c.trim() == "web_live");
    let mut web = Vec::new();
    files_under(&repo().join("web/test"), &mut web);
    for file in web {
        let source = std::fs::read_to_string(&file).unwrap_or_default();
        let named = file
            .strip_prefix(repo())
            .is_ok_and(|p| units.iter().any(|u| Path::new(u) == p));
        if reads_the_data_tree(&source) && !(web_live && named) {
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
    let code = code(source);
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

/// Whether a Rust source's code climbs to `path` from a crate (`../path`, `../../path/…`): a
/// read of this repository's own, whatever else the source names it as.
fn climbs_to(source: &str, path: &str) -> bool {
    code(source).contains(&format!("../{path}"))
}

/// A Rust source without its comment lines.
fn code(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The sources that name a skipped path only as a path of a scratch repository, of a checkout
/// handed over at run time, of a branch read through git, of a literal under test or of a
/// document a printed line points a person to, never to read this repository's own: each one
/// read by a person, so a new source naming one fails until it is read too.
const NAMED_AS_DATA: &[(&str, &str)] = &[
    ("app/cli/src/commands/doctor.rs", "docs"),
    ("app/cli/tests/support/fixture.rs", "docs"),
    ("app/cli/src/commands/init/onboarding.rs", "docs"),
    ("app/cli/src/commands/release.rs", "CHANGELOG.md"),
    ("app/cli/src/commands/release.rs", "changelog.d"),
    ("app/cli/tests/coding.rs", "docs"),
    ("app/cli/tests/mcp.rs", "docs"),
    ("app/cli/tests/release.rs", "CHANGELOG.md"),
    ("app/cli/tests/release.rs", "changelog.d"),
    ("app/inbox/src/coding.rs", "docs"),
    ("app/held/src/leak.rs", "docs"),
    ("app/base/src/repo_ids.rs", "docs"),
    ("app/inbox/tests/act_literal.rs", "docs"),
    ("app/server/tests/web_vectors.rs", "docs"),
    ("app/io/src/repos.rs", "docs"),
    // `memory` is also the data tree's store of the secretary's memory and its route's segment.
    ("app/cli/tests/support/fixture.rs", "memory"),
    ("app/server/src/serve/table.rs", "memory"),
    ("app/cli/src/wiring.rs", "memory"),
    ("app/cli/tests/common/mod.rs", "memory"),
    ("app/cli/tests/memory.rs", "memory"),
    ("app/cli/tests/resident.rs", "memory"),
    ("app/cli/tests/signatures.rs", "memory"),
    ("app/cli/tests/stores.rs", "memory"),
    ("app/cli/tests/writes.rs", "memory"),
    ("app/inbox/src/secretary/card.rs", "memory"),
    ("app/io/src/objects.rs", "memory"),
    ("app/pages/src/places.rs", "memory"),
    ("app/pages/tests/laws/memory.rs", "memory"),
    // The gate run in a throwaway checkout writes a file under each path a step may skip.
    ("app/script-guards/tests/guards/gate_runs.rs", "android"),
    (
        "app/script-guards/tests/guards/gate_runs.rs",
        "app/api/proptest-regressions",
    ),
    ("app/script-guards/tests/guards/gate_runs.rs", "app/io/src"),
    (
        "app/script-guards/tests/guards/gate_runs.rs",
        "app/pages/src",
    ),
    (
        "app/script-guards/tests/guards/gate_runs.rs",
        "app/secretary/src",
    ),
    ("app/script-guards/tests/guards/gate_runs.rs", "docs"),
    ("app/script-guards/tests/guards/gate_runs.rs", "web/e2e"),
    ("app/script-guards/tests/guards/gate_runs.rs", "web/src"),
    ("app/script-guards/tests/guards/gate_runs.rs", "web/test"),
    ("app/script-guards/tests/guards/gate_runs.rs", "web/typst"),
    // A scratch home's assistant memory, which the vault backs up.
    ("app/script-guards/tests/guards/vault.rs", "memory"),
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
/// source it builds or runs may name that path, and none may climb to it from a crate; a path
/// only a declaring unit reads may be named by the sources of a member whose unit declares it.
/// The `script-guards` step builds its own crate alone, so its skips hold that crate's sources;
/// the other steps' skips hold every other source but the CLI's guards', which the `guards` step
/// reads with the whole tree.
#[test]
fn merge_gate_no_rust_source_names_a_path_its_cargo_steps_skip() {
    let skips = |steps: &[Step]| -> BTreeSet<String> {
        steps
            .iter()
            .flat_map(|s| s.skips().into_iter().map(str::to_owned).collect::<Vec<_>>())
            .collect()
    };
    let (script_guards, others): (Vec<Step>, Vec<Step>) = steps()
        .into_iter()
        .filter(|s| s.command.contains("cargo"))
        .partition(|s| s.name == "script-guards");
    let (own, skipped) = (skips(&script_guards), skips(&others));
    assert!(!skipped.is_empty(), "the cargo steps skip the web");
    assert!(
        own.contains("app/cli/src"),
        "the script guards skip the CLI's code"
    );
    let script_guards = repo().join("app/script-guards");
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
        let skipped = if file.starts_with(&script_guards) {
            &own
        } else {
            &skipped
        };
        for path in skipped {
            let climbs = climbs_to(&source, path);
            if !climbs
                && (NAMED_AS_DATA.contains(&(relative.as_ref(), path.as_str()))
                    || (cli_only.contains(path) && member.is_some_and(|m| declares(m, path))))
            {
                continue;
            }
            if climbs || names_path(&source, path) {
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

/// A kept scene reads nothing, so a scene is kept across a change that build-web runs for only
/// when its key holds every input of the bundle build-web writes: no path an e2e unit skips is
/// one the bundle is built from, but the inputs a scene reads once it fetched them
/// (`merge_gate_a_scene_is_kept_by_what_it_fetched`).
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
            "app/secretary",
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
/// The dry push runs the binary that build made, the program the deploy runs: this gate's, handed
/// on in its lane's shell, never a link another checkout's gate moves, and none at all after a
/// failed build. The leak check reads that build's tree with the deployed app, so no tree
/// weakens the matcher that reads it.
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
        1,
        "push-dry runs that binary, or fails"
    );
    assert!(
        gate.contains("bin=\"$(scripts/app-bin.sh --deployed)/bin/suzatary\" || exit"),
        "public-check's matcher is the deployed app"
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

/// The gate roots the environment it runs in before it builds anything: `nix develop` roots
/// nothing, and the daemon's collection, short of disk, took gitleaks from under the guards
/// mid-gate. So the rooting comes before the disk is checked, the gate collects nothing itself,
/// and what it roots holds every store path on the running shell's PATH in its closure, which is
/// what a collection keeps.
#[test]
fn merge_gate_roots_the_shell_it_runs_in_before_it_collects() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let line = |needle: &str| {
        gate.lines()
            .position(|l| l.trim_start().starts_with(needle))
            .unwrap_or_else(|| panic!("the gate has no line starting {needle:?}"))
    };
    assert!(
        line("if ! root_environment") < line("if [ \"$(free_gb)\" -lt 16 ]"),
        "the environment is rooted before the disk is checked"
    );
    assert!(
        !gate
            .lines()
            .any(|l| l.contains("nix-collect-garbage") && !l.trim_start().starts_with('#')),
        "the gate collects nothing itself"
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

/// Every path the gate's read lists name is in the tree: a step's pass is keyed by the paths it
/// reads, so a listed path that holds nothing keys it by nothing, and the step is kept stale once
/// something lands there. The lists are each step's reads, `crate_reads` and the scenes' base
/// (`unit_base[files:web/e2e]`, which `--list` does not print). The private history may be
/// absent: a public tree leaves it out, and `docs/reports` is empty between sessions.
#[test]
fn merge_gate_every_path_a_read_list_names_is_in_the_tree() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let scenes: Vec<String> = gate
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("[files:web/e2e]=\"")?
                .strip_suffix('"')
        })
        .expect("the scenes' base")
        .split_whitespace()
        .filter(|word| !word.contains(['$', '{', '}']))
        .map(str::to_owned)
        .collect();
    let steps = steps();
    let listed = steps
        .iter()
        .flat_map(|step| {
            step.skips()
                .into_iter()
                .chain(step.only().unwrap_or_default())
        })
        .map(str::to_owned)
        .chain(
            crate_reads()
                .into_iter()
                .filter_map(|declared| declared.split_once('=').map(|(_, path)| path.to_owned())),
        )
        .chain(scenes);
    let private: Vec<String> = PRIVATE_HISTORY.iter().map(|&p| private_path(p)).collect();
    let missing: BTreeSet<String> = listed
        .filter(|path| !private.contains(path) && !repo().join(path).exists())
        .collect();
    assert!(
        missing.is_empty(),
        "scripts/merge-gate.sh's read lists name paths the tree does not hold: {missing:?}"
    );
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
            for include in super::includes::includes(&repo().join(&dir).join("src")) {
                if let Ok(target) = include.target().canonicalize()
                    && let Ok(rel) = target.strip_prefix(repo().canonicalize().expect("repo"))
                {
                    need(rel.to_string_lossy().into_owned());
                }
            }
        }
    }
    need("app/cli/tests/live.rs".to_owned());
    need("app/cli/tests/all.rs".to_owned());
    let all = std::fs::read_to_string(repo().join("app/cli/tests/all.rs")).unwrap_or_default();
    if all.lines().any(|line| line.trim() == "mod common;") {
        need("app/cli/tests/common".to_owned());
    }
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
                        .filter(|p| !["docs", "memory", "android"].contains(p));
                    !["docs", "memory", "android"]
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

/// The crates the core's library is cut into; a new one joins with its word in `flake.nix`'s
/// `library`.
const LIBRARY_CRATES: [&str; 24] = [
    "app/base",
    "app/record",
    "app/release",
    "app/store",
    "app/signatures",
    "app/todo",
    "app/plan",
    "app/held",
    "app/memory",
    "app/mail",
    "app/health",
    "app/triage",
    "app/services",
    "app/google",
    "app/connect",
    "app/inbox",
    "app/conversations",
    "app/lines",
    "app/topics",
    "app/digest",
    "app/stores",
    "app/cards",
    "app/secretary",
    "app/api",
];

/// Whether `rel` is a test file of one of the library's crates.
fn a_library_test(rel: &str) -> bool {
    LIBRARY_CRATES
        .iter()
        .any(|krate| rel.starts_with(&format!("{krate}/tests/")))
}

/// The browser module, the wire and the vectors are built from the core's library alone, so a
/// change to the CLI, the server's crates or a test file builds none of them again: their
/// sources hold, beside the workspace's manifests, the library of each crate the core is cut into
/// ([`LIBRARY_CRATES`]), the core's examples and release plan, the browser module's crate, the
/// calendar's schema, and of those crates' tests only the supports their libraries include, each
/// of which they hold.
#[test]
fn merge_gate_web_inputs_are_built_from_the_core_library_alone() {
    if !on_path("nix") || std::env::var_os("IN_NIX_SHELL").is_none() {
        eprintln!(
            "merge_gate_web_inputs_are_built_from_the_core_library_alone: not in a Nix shell"
        );
        return;
    }
    let library: Vec<String> = LIBRARY_CRATES
        .iter()
        .map(|krate| format!("{krate}/src/"))
        .chain(["app/todo/examples/".to_owned(), "app/wasm/".to_owned()])
        .collect();
    let files = ["app/Cargo.lock", "app/plugin-ics/ics.schema"];
    let mut sources = Vec::new();
    for krate in LIBRARY_CRATES {
        files_under(&repo().join(krate).join("src"), &mut sources);
    }
    // A `#[path]` is resolved from the directory of the file that declares it, as rustc does.
    let supports: BTreeSet<String> = sources
        .iter()
        .flat_map(|file| {
            let dir = file
                .parent()
                .expect("a source file has a directory")
                .to_owned();
            std::fs::read_to_string(file)
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.trim().strip_prefix("#[path = \"")?.strip_suffix("\"]"))
                .map(|rel| {
                    dir.join(rel)
                        .components()
                        .fold(PathBuf::new(), |mut at, part| {
                            match part {
                                std::path::Component::ParentDir => {
                                    at.pop();
                                }
                                other => at.push(other),
                            }
                            at
                        })
                })
                .filter_map(|path| {
                    Some(
                        path.strip_prefix(repo())
                            .ok()?
                            .to_string_lossy()
                            .into_owned(),
                    )
                })
                .filter(|rel| a_library_test(rel))
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
/// so web-inputs roots each one the store holds (`nix_held`), on the box as on a runner: a
/// collection that took one cost a gate the wasm32 dependencies again. One the store does not
/// hold (the modules were fetched) is not built only to be rooted.
#[test]
fn merge_gate_roots_the_web_inputs_dependency_builds() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let start = gate.find("\nweb_inputs() {").expect("the web inputs");
    let body = &gate[start..start + gate[start..].find("\n}").expect("its end")];
    let rooted = body
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("for deps in ")?
                .split_once("; do nix_held \"$deps\"")
        })
        .map(|(names, _)| names.split_whitespace().collect::<Vec<_>>())
        .expect("web-inputs roots the dependency builds it holds");
    for deps in ["suzatary-wasm-deps", "suzatary-model-deps"] {
        assert!(rooted.contains(&deps), "web-inputs roots {deps}");
    }
}

/// The string literals of `source` that name a path of the repository from its root.
fn repo_paths(source: &str) -> Vec<String> {
    source
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| {
            ["app/", "web/", "docs/", "memory/", "android/", "scripts/"]
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
        // A path named `<test>=<path>` is read by that test alone, so it must name one.
        for entry in shared
            .iter()
            .filter_map(|s| s.split_once('=').map(|(t, _)| t))
        {
            if !repo().join(entry).is_file() {
                wrong.push(format!("unit_shared names {entry}, which is no test file"));
            }
        }
        for file in files
            .iter()
            .filter(|f| f.extension().is_some_and(|e| e == "ts"))
        {
            let name = file
                .strip_prefix(repo())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let shared = shared
                .iter()
                .filter_map(|s| match s.split_once('=') {
                    None => Some(s.as_str()),
                    Some((test, path)) => (test == name).then_some(path),
                })
                .collect::<Vec<_>>();
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

/// A release commit changes `app/release.toml` and `CHANGELOG.md` and removes `changelog.d/` alone,
/// so it reruns only what reads them: the steps that read every file, and the one crate unit that declares the release
/// file (`suzatary`, whose guards hold the changelog's tests); no unit reads the changelog, and
/// the web's units skip both by their base.
#[test]
fn merge_gate_a_release_reruns_only_what_reads_it() {
    let release = [
        "app/release.toml",
        "CHANGELOG.md",
        "changelog.d/a-change.md",
    ];
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
    files_under(&repo().join("app/api/src/wire"), &mut sources);
    sources.push(repo().join("app/api/src/wire.rs"));
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

/// A scratch repository for the changelog step: `master` at a base commit holding `app/x` and a
/// changelog, with `origin/master` there.
struct ChangelogRepo(Scratch);

const OLD_CHANGELOG: &str = "# C\n\n## [Unreleased]\n\n## [1.0.0] - 2026-10-01\n\n- old\n";
const NEW_CHANGELOG: &str = "# C\n\n## [Unreleased]\n\n## [1.1.0] - 2026-10-14\n\n- new\n\n## [1.0.0] - 2026-10-01\n\n- old\n";

impl ChangelogRepo {
    fn new(tag: &str) -> Self {
        let repo = Self(Scratch::new(tag));
        repo.git(&["init", "-q", "-b", "master"]);
        repo.commit(
            "base",
            &[("CHANGELOG.md", OLD_CHANGELOG), ("app/x", "0\n")],
            &[],
        );
        repo.publish();
        repo
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(&*self.0)
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
    }

    /// A commit writing `writes` and removing `removes`; `message` holds its trailers.
    fn commit(&self, message: &str, writes: &[(&str, &str)], removes: &[&str]) {
        for (path, text) in writes {
            let path = self.0.join(path);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
            std::fs::write(path, text).expect("writes");
        }
        for path in removes {
            self.git(&["rm", "-q", path]);
        }
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
    }

    /// A release commit: it writes `changelog` and folds `fragments`.
    fn release(&self, version: &str, changelog: &str, fragments: &[&str]) {
        self.commit(
            &format!("release: {version}\n\nRelease: {version}"),
            &[("CHANGELOG.md", changelog)],
            fragments,
        );
    }

    /// `origin/master` moved to HEAD, as the nightly sees it.
    fn publish(&self) {
        self.git(&["update-ref", "refs/remotes/origin/master", "HEAD"]);
    }

    fn step(&self) -> bool {
        Command::new("bash")
            .arg("-c")
            .arg(format!(
                "set -euo pipefail; source <(sed -n '/^changelog_entry()/,/^}}/p' {}); changelog_entry",
                repo().join("scripts/merge-gate.sh").display()
            ))
            .current_dir(&*self.0)
            .output()
            .expect("bash runs")
            .status
            .success()
    }
}

/// An `app/` or `web/` change needs a fragment, read on the commit before a release, which folds
/// the fragments and so would read as none added.
#[test]
fn the_changelog_step_wants_a_fragment_beside_a_change() {
    let repo = ChangelogRepo::new("gate-changelog-fragment");
    repo.commit(
        "Merge #1: a change",
        &[
            ("app/x", "1\n"),
            ("changelog.d/one.md", "### Added\n- new\n"),
        ],
        &[],
    );
    assert!(repo.step(), "a fragment rides the change");
    let bare = ChangelogRepo::new("gate-changelog-no-fragment");
    bare.commit("Merge #2: no entry", &[("app/x", "2\n")], &[]);
    bare.release("1.1.0", NEW_CHANGELOG, &[]);
    assert!(
        !bare.step(),
        "a train with no entry fails even under its release"
    );
}

/// This tree's own fragments, read by this tree's binary as a cut reads them: where a fragment is
/// written, the gate's step has no app to read it with, and a train's cut removes it first.
#[test]
fn this_tree_s_fragments_are_read_as_a_cut_reads_them() {
    let out = Command::new(binary::path())
        .args(["release", "fragments", "--tree"])
        .arg(repo())
        .output()
        .expect("the binary runs");
    assert!(
        out.status.success(),
        "a fragment a release cut would refuse:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Every fragment is read by origin/master's app as the release cut reads it, whatever changed:
/// one under no `### <kind>` heading fails the step, even with no `app/` or `web/` change.
#[test]
fn the_changelog_step_reads_every_fragment_as_the_cut_does() {
    let repo = ChangelogRepo::new("gate-changelog-parse");
    let app = repo.0.join("app-out");
    std::fs::create_dir_all(app.join("bin")).expect("a bin dir");
    let real = std::fs::canonicalize(binary::path()).expect("the binary");
    std::os::unix::fs::symlink(real, app.join("bin/suzatary")).expect("the app");
    repo.commit(
        "the cut's app, as scripts/app-bin.sh prints it",
        &[(
            "scripts/app-bin.sh",
            &format!("#!/bin/sh\necho '{}'\n", app.display()),
        )],
        &[],
    );
    std::fs::set_permissions(
        repo.0.join("scripts/app-bin.sh"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .expect("executable");
    repo.commit(
        "a fragment",
        &[("changelog.d/one.md", "### Fixed\n- one\n")],
        &[],
    );
    assert!(repo.step(), "a fragment the cut reads");
    repo.commit(
        "a bare fragment",
        &[("changelog.d/two.md", "- under no kind\n")],
        &[],
    );
    assert!(
        !repo.step(),
        "a fragment the cut refuses, with no app/ change"
    );
}

/// A release commit at the tip of a train writes `CHANGELOG.md`, and the step passes it.
#[test]
fn the_changelog_step_passes_a_release_commit() {
    let repo = ChangelogRepo::new("gate-changelog-release");
    repo.commit(
        "Merge #1: a change",
        &[
            ("app/x", "1\n"),
            ("changelog.d/one.md", "### Added\n- new\n"),
        ],
        &[],
    );
    repo.release("1.1.0", NEW_CHANGELOG, &["changelog.d/one.md"]);
    assert!(repo.step());
}

/// Any commit but a release that writes `CHANGELOG.md` fails the step.
#[test]
fn the_changelog_step_refuses_a_non_release_edit_of_the_changelog() {
    let repo = ChangelogRepo::new("gate-changelog-edit");
    repo.commit(
        "Merge #4: an entry in the changelog",
        &[
            ("app/x", "4\n"),
            ("changelog.d/four.md", "### Fixed\n- four\n"),
            ("CHANGELOG.md", "# C\n\n## [Unreleased]\n\n- four\n"),
        ],
        &[],
    );
    assert!(!repo.step());
}

/// On master itself (the nightly), HEAD is origin/master, and the step reads the train since the
/// release before HEAD: master's newest release passes, and a hand edit after it fails.
#[test]
fn the_changelog_step_on_master_reads_the_train_since_the_last_release() {
    let repo = ChangelogRepo::new("gate-changelog-master");
    repo.release("1.0.0", OLD_CHANGELOG, &[]);
    repo.commit(
        "Merge #5: a change",
        &[
            ("app/x", "5\n"),
            ("changelog.d/five.md", "### Fixed\n- five\n"),
        ],
        &[],
    );
    repo.release("1.1.0", NEW_CHANGELOG, &["changelog.d/five.md"]);
    repo.publish();
    assert!(repo.step(), "master's newest release writes CHANGELOG.md");
    repo.commit(
        "an entry in the changelog",
        &[("CHANGELOG.md", "# C\n\n## [Unreleased]\n\n- by hand\n")],
        &[],
    );
    repo.publish();
    assert!(!repo.step(), "only a release writes CHANGELOG.md on master");
}

/// Every script the gate, the train or a workflow runs by its path is executable in the index: a
/// checkout takes its mode from there, and a script that lost it fails every step that runs it,
/// on a runner, long after the commit. A script sourced (`.`, `source`) or handed to an
/// interpreter (`bash`, `sh`, `node`) needs no mode.
#[test]
fn merge_gate_every_script_run_by_path_is_executable() {
    let modes = Command::new("git")
        .args(["ls-files", "-s", "scripts"])
        .current_dir(repo())
        .output()
        .expect("git runs");
    let modes = String::from_utf8_lossy(&modes.stdout);
    let mode_of = |path: &str| {
        modes
            .lines()
            .find(|l| l.ends_with(&format!("\t{path}")))
            .map(|l| l.split_whitespace().next().unwrap_or_default().to_owned())
    };
    let mut runners = vec![
        "scripts/merge-gate.sh".to_owned(),
        "scripts/train.sh".to_owned(),
    ];
    for entry in std::fs::read_dir(repo().join(".github/workflows")).expect("the workflows") {
        let name = entry.expect("a workflow").file_name();
        runners.push(format!(".github/workflows/{}", name.to_string_lossy()));
    }
    let mut lost = Vec::new();
    for runner in &runners {
        let text = std::fs::read_to_string(repo().join(runner)).expect("reads");
        for line in text.lines() {
            let code = line.split(" #").next().unwrap_or_default();
            if code.trim_start().starts_with('#') {
                continue;
            }
            let words: Vec<&str> = code.split_whitespace().collect();
            for (i, word) in words.iter().enumerate() {
                let Some(at) = word.find("scripts/") else {
                    continue;
                };
                let path: String = word[at..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || "/._-".contains(*c))
                    .collect();
                let interpreted =
                    i > 0 && [".", "source", "bash", "sh", "node"].contains(&words[i - 1]);
                if Path::new(&path).extension().is_none_or(|e| e != "sh") || interpreted {
                    continue;
                }
                if mode_of(&path).as_deref() != Some("100755") {
                    lost.push(format!("{runner} runs {path}"));
                }
            }
        }
    }
    lost.sort();
    lost.dedup();
    assert!(
        lost.is_empty(),
        "these scripts are run by path and are not 100755 in the index: {lost:?}"
    );
}

/// The private history, as `.gitattributes` marks it export-ignore, each path as its directory and
/// its name; the public tree holds neither these paths nor the attributes, and no line here spells
/// a private file's path whole, which the leak guard would read as a link to it.
const PRIVATE_HISTORY: &[(&str, &str)] = &[
    ("docs", "reports"),
    ("docs", "CLAUDE-history.md"),
    ("", "CHANGELOG.md"),
    ("", "changelog.d"),
    ("", ".gitattributes"),
    ("app/cli/tests", "live.rs"),
];

/// A private path, whole.
fn private_path((dir, name): (&str, &str)) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// A source's top-level functions: (name, whether it is a test, its body).
fn functions(source: &str) -> Vec<(String, bool, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let Some(rest) = ["fn ", "pub fn ", "pub(super) fn ", "pub(crate) fn "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
        else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let test = at > 0 && lines[at - 1].trim() == "#[test]";
        let tail = lines[at..].join("\n");
        let (mut depth, mut end) = (0_i32, tail.len());
        for (i, c) in tail.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.push((name, test, tail[..end].to_owned()));
    }
    found
}

/// Whether a function's body reads beyond the public tree: this deployment, a path of the private
/// history, or this checkout itself (git run in it, or a walk of its directories).
fn reads_beyond_the_public_tree(body: &str) -> bool {
    let deployment = body.contains("this_box::")
        || body.split("public_tree(").skip(1).any(|call| {
            call.split(';')
                .next()
                .is_some_and(|c| c.contains(", None)"))
        });
    let private = PRIVATE_HISTORY.iter().any(|&(dir, name)| {
        body.contains(&format!("\"{}\"", private_path((dir, name))))
            || body.contains(&format!("\"{name}\""))
    });
    let git = body.contains("Command::new(\"git\")")
        && (body.contains(".arg(repo())") || body.contains("current_dir(repo())"));
    let walk = ["read_dir(", "files_under(", "walk("]
        .iter()
        .any(|w| body.contains(w))
        && body.contains("repo()");
    // The box's own `.#triage`, asked with nothing else on PATH: what the train runs in here.
    let shell = body.contains("\"--ignore-environment\"");
    deployment || private || git || walk || shell
}

/// The live half runs on the box exactly the guards whose verdict the public tree cannot give:
/// those that read this deployment, the private history or this checkout's own history and
/// directories, directly or through a helper of their file or of their target's guards.rs, in the
/// CLI's guards or the script guards. GitHub's guards lanes run every guard over the public tree,
/// so every other guard's verdict is already the tree's. `app/cli/tests/guards/on-the-box.txt` is
/// the list, and the reading here is its definition.
#[test]
fn merge_gate_live_guards_are_the_guards_that_read_beyond_the_public_tree() {
    if let Ok(attributes) = std::fs::read_to_string(repo().join(".gitattributes")) {
        let mut marked: Vec<String> = attributes
            .lines()
            .filter_map(|l| l.strip_suffix(" export-ignore"))
            .map(|p| p.trim_start_matches('/').trim_end_matches("/**").to_owned())
            .collect();
        marked.sort();
        let mut pinned: Vec<String> = PRIVATE_HISTORY.iter().map(|&p| private_path(p)).collect();
        pinned.sort();
        assert_eq!(
            marked, pinned,
            "PRIVATE_HISTORY is .gitattributes' export-ignore"
        );
    }
    let mut beyond = BTreeSet::new();
    for tests in ["app/cli/tests", "app/script-guards/tests"] {
        let tests = repo().join(tests);
        let root = std::fs::read_to_string(tests.join("guards.rs")).expect("guards.rs reads");
        let mut files = vec![(String::new(), root.clone())];
        let mut modules: Vec<PathBuf> = std::fs::read_dir(tests.join("guards"))
            .expect("guards/")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .collect();
        modules.sort();
        for module in modules {
            let name = module.file_stem().expect("a stem").to_string_lossy();
            let source = std::fs::read_to_string(&module).expect("a guard module reads");
            files.push((format!("{name}::"), source));
        }
        let shared = functions(&root);
        for (module, source) in &files {
            let own = functions(source);
            let helpers: Vec<&str> = own
                .iter()
                .chain(&shared)
                .filter(|(_, test, body)| !test && reads_beyond_the_public_tree(body))
                .map(|(name, _, _)| name.as_str())
                .collect();
            for (name, test, body) in &own {
                let calls = helpers.iter().any(|h| {
                    h != name
                        && body.match_indices(&format!("{h}(")).any(|(i, _)| {
                            !body[..i]
                                .ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '.')
                        })
                });
                if *test && (reads_beyond_the_public_tree(body) || calls) {
                    beyond.insert(format!("{module}{name}"));
                }
            }
        }
    }
    let listed: BTreeSet<String> =
        std::fs::read_to_string(repo().join("app/cli/tests/guards/on-the-box.txt"))
            .expect("the box's guards are listed")
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect();
    assert_eq!(
        listed, beyond,
        "app/cli/tests/guards/on-the-box.txt lists the guards that read beyond the public tree"
    );
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    assert!(
        gate.contains("guards/on-the-box.txt"),
        "the live half runs the listed guards"
    );
}
