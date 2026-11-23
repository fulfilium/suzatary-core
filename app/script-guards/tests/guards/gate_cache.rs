//! The merge gate's builds of the public tree under the train's `SUZATARY_GATE_BUILDS=cache`, run
//! for real in a throwaway checkout with a recording `nix`: every build is a fetch from a binary
//! cache (`--max-jobs 0`), never a build on the machine, and a miss at any of them ends the gate
//! with no verdict (75), not a red row of the tree. Beside it, `app-builds.sh` runs alone from
//! another directory, as the roots unit runs it from the store.

use super::scratch::{Scratch, repo};
use std::process::{Command, Output};

/// The recording `nix`: it logs each call; it adds a tree to a fake store by copying it, names a
/// system, and answers a build with a store path, except a build naming `$MISS`, which fails, and
/// whose dry run says it would be built, as a miss does under `--max-jobs 0`.
const NIX: &str = r#"#!/bin/sh
echo "nix $*" >>"$NIX_CALLS"
case "$1 $2" in
  "store add-path")
    n="$(ls "$FAKE_STORE" | wc -l)"
    cp -r "$5" "$FAKE_STORE/$n-source" && echo "$FAKE_STORE/$n-source" ;;
  "eval --impure") echo x86_64-linux ;;
  "build "*)
    case " $* " in
      *" --dry-run "*) echo "these 1 derivations will be built:" >&2 ;;
      *"$MISS"*) [ -z "$MISS" ] && mkdir -p "$FAKE_STORE/built/bin" && echo "$FAKE_STORE/built" || exit 1 ;;
      *) mkdir -p "$FAKE_STORE/built/bin" && echo "$FAKE_STORE/built" ;;
    esac ;;
esac
"#;

/// The recording `nix-store`: no path is valid, and rooting or realising a path succeeds.
const NIX_STORE: &str = r#"#!/bin/sh
echo "nix-store $*" >>"$NIX_CALLS"
[ "$1" != --check-validity ]
"#;

/// The tools `merge-gate.sh` asks for before it starts; those the steps never run are stand-ins
/// where this machine lacks them.
const TOOLS: [&str; 20] = [
    "setsid",
    "setpriv",
    "script",
    "curl",
    "cargo",
    "rustfmt",
    "cargo-clippy",
    "mold",
    "node",
    "esbuild",
    "tsc",
    "chromium",
    "brotli",
    "typst",
    "pdftotext",
    "gitleaks",
    "actionlint",
    "notmuch",
    "gpg",
    "journalctl",
];

/// The scripts whose builds of the public tree, and of the nixpkgs tools it runs with, are the box
/// steps of a gate on GitHub's runners.
const FETCHING: [&str; 3] = ["app-builds.sh", "deployed-shape.sh", "merge-gate.sh"];

/// A directory of the recording `nix` and `nix-store`, and a stand-in for each tool this machine
/// lacks; the PATH that puts it first.
fn stand_ins(scratch: &Scratch) -> std::ffi::OsString {
    let bin = scratch.join("bin");
    std::fs::create_dir_all(scratch.join("store")).expect("the fake store");
    let _ = suzatary_testkit::fake_program(&bin, "nix", NIX);
    let _ = suzatary_testkit::fake_program(&bin, "nix-store", NIX_STORE);
    for tool in TOOLS {
        if !super::scratch::on_path(tool) {
            let _ = suzatary_testkit::fake_program(&bin, tool, "#!/bin/sh\nexit 0\n");
        }
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&path))).expect("a PATH")
}

/// A checkout holding the gate, the scripts its public-tree steps run and a tree they read.
fn checkout(scratch: &Scratch) -> std::path::PathBuf {
    let checkout = scratch.join("checkout");
    std::fs::create_dir_all(checkout.join("scripts")).expect("the checkout");
    for script in FETCHING
        .iter()
        .chain(&["groups.sh", "nix-out.sh", "public-tree.sh"])
    {
        let bytes = std::fs::read(repo().join("scripts").join(script)).expect("reads");
        suzatary_testkit::write_program(&checkout.join("scripts").join(script), &bytes);
    }
    for (rel, text) in [
        ("app/Cargo.toml", "version = \"0.1.0\"\n"),
        ("CHANGELOG.md", "# Changelog\n"),
        ("scripts/public-changelog.md", "@VERSION@ @DATE@\n"),
        ("nix/app-input.nix", "{ }\n"),
        ("flake.nix", "{ }\n"),
    ] {
        let path = checkout.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(path, text).expect("writes");
    }
    for args in [
        &["init", "-q", "-b", "master"][..],
        &["add", "-A"],
        &["commit", "-q", "-m", "seed"],
    ] {
        let out = Command::new("git")
            .args(args)
            .current_dir(&checkout)
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
    checkout
}

/// The gate's `steps` in a fresh checkout, with `builds` as `SUZATARY_GATE_BUILDS` and every build
/// naming `miss` failing as a miss.
fn gate(name: &str, steps: &str, builds: Option<&str>, miss: &str) -> (Scratch, Output) {
    let scratch = Scratch::new(name);
    let checkout = checkout(&scratch);
    let mut cmd = Command::new(checkout.join("scripts/merge-gate.sh"));
    cmd.args(["--no-live", "--steps", steps])
        .current_dir(&checkout)
        .env("PATH", stand_ins(&scratch))
        .env("NIX_CALLS", scratch.join("calls"))
        .env("FAKE_STORE", scratch.join("store"))
        .env("MISS", miss)
        .env("XDG_STATE_HOME", scratch.join("state"))
        .env_remove("SUZATARY_GATE_RUN")
        .env_remove("SUZATARY_GATE_BUILDS");
    if let Some(builds) = builds {
        cmd.env("SUZATARY_GATE_BUILDS", builds);
    }
    let out = cmd.output().expect("merge-gate.sh runs");
    (scratch, out)
}

/// The `nix build` calls a run made, its dry runs left out.
fn nix_builds(scratch: &Scratch) -> Vec<String> {
    std::fs::read_to_string(scratch.join("calls"))
        .unwrap_or_default()
        .lines()
        .filter(|call| call.starts_with("nix build ") && !call.contains("--dry-run"))
        .map(str::to_owned)
        .collect()
}

/// Each `nix build` of the public tree (`$app#`, `$tree#`) or of nixpkgs in the scripts of the box's
/// steps, its continuation lines joined: a fetch under `--cached`, its failure asked of `missed`.
#[test]
fn every_build_of_the_box_steps_is_a_fetch_and_its_failure_asks_missed() {
    for script in FETCHING {
        let text = std::fs::read_to_string(repo().join("scripts").join(script)).expect("reads");
        let joined = text.replace("\\\n", " ").replace("||\n", "|| ");
        for line in joined.lines().filter(|line| {
            line.contains("nix build")
                && ["$app#", "$tree#", "nixpkgs#", "\"${tools[@]}\""]
                    .iter()
                    .any(|named| line.contains(named))
        }) {
            assert!(
                line.contains("\"${fetched[@]}\"") && line.contains("missed"),
                "{script}: a build of the box's steps that is no fetch: {line}"
            );
        }
    }
}

/// Under `cache`, a miss at each fetch the steps reach ends the gate with no verdict and makes no
/// build on the machine; without it, the same miss is a build that failed, red.
#[test]
fn a_miss_at_any_fetch_of_a_gate_for_the_runners_is_no_verdict() {
    for (steps, miss) in [
        ("public-tree", "#suzatary "),
        ("public-tree", "#suzatary-web"),
        ("public-tree", "#suzatary-plugins"),
        ("deployed-shape", "#suzatary "),
        ("deployed-shape", "nixpkgs#bash"),
    ] {
        let (scratch, out) = gate("gate-cache", steps, Some("cache"), miss);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(75), "{steps} {miss}: {stderr}");
        assert!(
            stderr.contains("missed (75)") && stderr.contains("then gate again"),
            "{steps} {miss}: {stderr}"
        );
        for call in nix_builds(&scratch) {
            assert!(
                call.contains("--max-jobs 0"),
                "a build on the machine: {call}"
            );
        }
        assert!(
            !std::fs::read_to_string(scratch.join("calls"))
                .unwrap_or_default()
                .contains("nix develop"),
            "the cached mode makes no shell"
        );
    }
    let (scratch, out) = gate("gate-builds", "public-tree", None, "#suzatary-web");
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        nix_builds(&scratch)
            .iter()
            .all(|call| !call.contains("--max-jobs 0")),
        "the gate on the box builds"
    );
}

/// The roots unit runs `bash ${../scripts/app-builds.sh}`: the file alone in the store, from
/// another directory, never with --cached. It needs no file beside it.
#[test]
fn app_builds_runs_alone_as_the_roots_unit_runs_it() {
    let scratch = Scratch::new("app-builds-alone");
    let alone = scratch.join("store-copy");
    std::fs::create_dir_all(&alone).expect("dirs");
    let script = alone.join("app-builds.sh");
    let bytes = std::fs::read(repo().join("scripts/app-builds.sh")).expect("reads");
    std::fs::write(&script, bytes).expect("writes");
    let app = scratch.join("app");
    std::fs::create_dir_all(app.join("nix")).expect("dirs");
    std::fs::write(app.join("nix/app-input.nix"), "{ }\n").expect("writes");
    let elsewhere = scratch.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("dirs");
    let out = Command::new("bash")
        .arg(&script)
        .arg(&app)
        .arg(scratch.join("roots/lock-"))
        .current_dir(&elsewhere)
        .env("PATH", stand_ins(&scratch))
        .env("NIX_CALLS", scratch.join("calls"))
        .env("FAKE_STORE", scratch.join("store"))
        .env("MISS", "")
        .output()
        .expect("bash runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let calls = std::fs::read_to_string(scratch.join("calls")).unwrap_or_default();
    assert!(
        calls.contains("nix develop") && !calls.contains("--max-jobs 0"),
        "{calls}"
    );
}
