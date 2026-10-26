//! The adoption (`scripts/adopt-signatures.sh`), run as the real script against a throwaway
//! data repository and roots, with a stub `systemctl` whose units are files that exist while
//! active, and a stub deployed binary whose answers are set per test. Every call is logged in
//! order, so a test reads what the script did and when.

use super::{Scratch, repo};
use std::path::Path;
use std::process::{Command, Output};

/// `systemctl --user stop|start|is-active --quiet <unit>…`: a unit is active while
/// `<state>/units/<unit>` exists, and a unit named in `<state>/wont-start` never starts.
const SYSTEMCTL: &str = r#"#!/usr/bin/env bash
s="$ADOPT_STATE"
echo "systemctl $*" >>"$s/log"
verb="$2"
shift 2
[ "${1:-}" = --quiet ] && shift
for unit in "$@"; do
  case "$verb" in
    stop) rm -f "$s/units/$unit" ;;
    start) grep -qx "$unit" "$s/wont-start" 2>/dev/null || touch "$s/units/$unit" ;;
    is-active) [ -f "$s/units/$unit" ] || exit 3 ;;
  esac
done
"#;

/// The deployed binary: `--instance <file>` then a verb. It logs the verb and the code root it
/// was handed; `device sign-unsigned` fails while `<state>/unsigned` exists, and `instance` reads
/// and writes the root as the real verb does, by its key.
const SUZATARY: &str = r#"#!/usr/bin/env bash
s="$ADOPT_STATE"
file="$2"
shift 2
echo "suzatary $* repo=$SUZATARY_REPO" >>"$s/log"
case "$*" in
  "device --help") echo "  sign-unsigned  sign what was written unsigned" ;;
  "device init") echo "root 9999999999999999999999999999999999999999999999999999999999999999" ;;
  "device sign-unsigned") [ ! -f "$s/unsigned" ] || { echo "a write is unsigned" >&2; exit 1; } ;;
  doctor) echo "signatures: the box's and the agent's keys resolve" ;;
  "instance get signatures.root") [ -f "$s/root" ] || exit 1; cat "$s/root" ;;
  "instance set signatures.root "*) printf '\n[signatures]\nroot = "%s"\n' "$4" >>"$file"; echo "$4" >"$s/root" ;;
esac
"#;

const WRITERS: [&str; 3] = [
    "suzatary-web.service",
    "suzatary-triage.timer",
    "suzatary-mail-idle.service",
];

/// A data repository holding an instance file, a deployed tree and binary under `roots/`, the
/// writers running; answers the instance file's path.
fn world(root: &Path) -> std::path::PathBuf {
    let write = |path: &Path, text: &str| {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, text).expect("written");
    };
    let _ = suzatary_testkit::fake_program(&root.join("bin"), "systemctl", SYSTEMCTL);
    let _ = suzatary_testkit::fake_program(&root.join("roots/lock-bin/bin"), "suzatary", SUZATARY);
    std::fs::create_dir_all(root.join("store/app-tree")).expect("a tree");
    std::os::unix::fs::symlink(root.join("store/app-tree"), root.join("roots/app"))
        .expect("the app link");
    for unit in WRITERS {
        write(&root.join("units").join(unit), "");
    }
    let instance = root.join("data/instance.toml");
    write(&instance, "[deployment]\n");
    for args in [
        &["init", "-q"][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "add",
            "instance.toml",
        ],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "first",
        ],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(root.join("data"))
            .status()
            .expect("git runs");
        assert!(ok.success());
    }
    instance
}

fn adopt(root: &Path, instance: &Path, runtime: bool) -> Output {
    let mut command = Command::new("bash");
    command
        .arg(repo().join("scripts/adopt-signatures.sh"))
        .env(
            "PATH",
            format!(
                "{}:{}",
                root.join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("ADOPT_STATE", root)
        .env("SUZATARY_ROOTS", root.join("roots"))
        .env("SUZATARY_INSTANCE", instance)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env_remove("SUZATARY_REPO");
    if runtime {
        command.env("XDG_RUNTIME_DIR", root);
    } else {
        command.env_remove("XDG_RUNTIME_DIR");
    }
    command.output().expect("the script runs")
}

fn log(root: &Path) -> String {
    std::fs::read_to_string(root.join("log")).unwrap_or_default()
}

/// Every writer the module runs is one the script stops, the IMAP watcher (which starts the pass
/// early) among them, and its verbs go through `lock-bin`, never a checkout's build.
#[test]
fn adopt_signatures_stops_the_module_s_writers_and_runs_the_deployed_binary() {
    let script = std::fs::read_to_string(repo().join("scripts/adopt-signatures.sh"))
        .expect("the script reads");
    let services =
        std::fs::read_to_string(repo().join("nix/services.nix")).expect("the module reads");
    let writers = script
        .lines()
        .find(|line| line.starts_with("writers=("))
        .expect("the script lists the writers it stops");
    for unit in WRITERS {
        assert!(writers.contains(unit), "{unit} is not stopped: {writers}");
        let name = unit.split('.').next().expect("a unit name");
        assert!(
            services.contains(&format!("name = \"{name}\"")),
            "{name} is no unit of nix/services.nix"
        );
    }
    assert!(script.contains("lock-bin/bin/suzatary"));
    assert!(
        !script.contains("app-bin.sh"),
        "a checkout's build would adopt"
    );
}

/// With no runtime directory `systemctl --user` reaches no manager and answers empty, so the
/// script refuses before it asks anything.
#[test]
fn adopt_signatures_refuses_without_a_user_manager() {
    let root = Scratch::new("adopt-runtime");
    let instance = world(&root);
    let ran = adopt(&root, &instance, false);
    assert!(!ran.status.success());
    assert!(String::from_utf8_lossy(&ran.stderr).contains("XDG_RUNTIME_DIR"));
    assert_eq!(log(&root), "", "nothing was asked");
}

/// A whole run: the writers stopped before the root is declared and started after, the root
/// committed in the data repository, and every verb handed the deployed tree as its code root.
#[test]
fn adopt_signatures_declares_the_root_between_stopping_and_starting_the_writers() {
    let root = Scratch::new("adopt-whole");
    let instance = world(&root);
    let ran = adopt(&root, &instance, true);
    assert!(
        ran.status.success(),
        "{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let said = std::fs::read_to_string(&instance).expect("the instance");
    assert!(
        said.contains(&format!("root = \"{}\"", "9".repeat(64))),
        "{said}"
    );
    let committed = Command::new("git")
        .args(["show", "HEAD:instance.toml"])
        .current_dir(root.join("data"))
        .output()
        .expect("git runs");
    assert_eq!(String::from_utf8_lossy(&committed.stdout), said);
    let log = log(&root);
    let at = |what: &str| log.find(what).unwrap_or_else(|| panic!("{what} in {log}"));
    assert!(at("systemctl --user stop") < at("suzatary device sign-unsigned"));
    assert!(at("suzatary device sign-unsigned") < at("systemctl --user start"));
    assert!(
        at("systemctl --user start") < at("suzatary verify"),
        "what adoption left is verified as the push path verifies it"
    );
    let tree = std::fs::canonicalize(root.join("store/app-tree")).expect("the tree");
    for line in log.lines().filter(|line| line.starts_with("suzatary ")) {
        assert!(
            line.ends_with(&format!("repo={}", tree.display())),
            "{line}: the deployed tree is the code root"
        );
    }
    for unit in WRITERS {
        assert!(root.join("units").join(unit).exists(), "{unit} runs again");
    }
}

/// A writer that does not start at the end is a failure like any other: named with the command
/// that starts it.
#[test]
fn adopt_signatures_names_a_writer_that_did_not_start_at_the_end() {
    let root = Scratch::new("adopt-unstarted");
    let instance = world(&root);
    std::fs::write(root.join("wont-start"), "suzatary-triage.timer\n").expect("written");
    let ran = adopt(&root, &instance, true);
    assert!(!ran.status.success());
    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(
        stderr.contains("suzatary-triage.timer did not start"),
        "{stderr}"
    );
    assert!(
        stderr.contains("systemctl --user start suzatary-triage.timer"),
        "{stderr}"
    );
}

/// A failure after the writers stop starts them again, checks each, and names the command for
/// any that did not start.
#[test]
fn adopt_signatures_restarts_the_writers_on_a_failure_and_names_any_still_stopped() {
    let root = Scratch::new("adopt-failed");
    let instance = world(&root);
    std::fs::write(root.join("unsigned"), "").expect("written");
    std::fs::write(root.join("wont-start"), "suzatary-mail-idle.service\n").expect("written");
    let ran = adopt(&root, &instance, true);
    assert!(!ran.status.success());
    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(
        stderr.contains("systemctl --user start suzatary-mail-idle.service"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("systemctl --user start suzatary-web.service"),
        "{stderr}"
    );
    for unit in ["suzatary-web.service", "suzatary-triage.timer"] {
        assert!(root.join("units").join(unit).exists(), "{unit} runs again");
    }
}
