//! The one way a test runs the binary, from a clean environment, so a suite run by a unit or the
//! owner (`SUZATARY_INSTANCE`, the XDG directories, `HOME`) never reads that deployment;
//! `binary_spawns_only_here` in `suites.rs` fails on a spawn anywhere else.

use std::path::PathBuf;
use std::process::Command;

/// What a run keeps of this process's environment: where its tools are, and where scratch goes.
const KEPT: [&str; 2] = ["PATH", "TMPDIR"];

/// `suzatary` with the [`KEPT`] variables and an empty `HOME` and runtime directory of its own.
pub(crate) fn suzatary() -> Command {
    run(std::path::Path::new(path()))
}

/// The executable at `exe` (a copy of the binary), run as [`suzatary`] runs the binary.
pub(crate) fn run(exe: &std::path::Path) -> Command {
    let home = home();
    let mut command = Command::new(exe);
    command
        .env_clear()
        .env("HOME", &home)
        .env("XDG_RUNTIME_DIR", home.join("run"));
    for name in KEPT {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

/// `suzatary-dev`, the binary with the fixture's verbs (`fixture-chain`, `serve --whois-fixture`), run
/// as [`suzatary`] runs the binary.
#[cfg(feature = "fixture")]
pub(crate) fn fixture() -> Command {
    run(std::path::Path::new(env!("CARGO_BIN_EXE_suzatary-dev")))
}

/// [`suzatary`] holding the cast's keys, as a box holds its own in the secret store: every
/// deployment signs and names its mail by its dedup key, so a run that writes needs them.
pub(crate) fn keyed() -> Command {
    let mut command = suzatary();
    command
        .env(
            suzatary_signatures::signatures::BOX_KEY,
            suzatary_connect::cast::keys::ROOT,
        )
        .env(
            suzatary_signatures::signatures::AGENT_KEY,
            suzatary_connect::cast::keys::AGENT,
        )
        .env(
            suzatary_mail::mail::dedup_key::DEDUP_KEY,
            suzatary_connect::cast::keys::DEDUP,
        );
    command
}

/// The script at `script` under `bash`, run as [`keyed`] runs the binary, with `code` as its code
/// repository (`SUZATARY_REPO`), whose `scripts/app-bin.sh` this writes to run this build: a data
/// push, real or dry, verifies and records with it as with the deployed program.
pub(crate) fn keyed_script(script: &std::path::Path, code: &std::path::Path) -> Command {
    let app_bin = code.join("scripts").join("app-bin.sh");
    std::fs::create_dir_all(code.join("scripts")).expect("a code repository");
    std::fs::write(&app_bin, format!("#!/bin/sh\nexec '{}' \"$@\"\n", path())).expect("app-bin.sh");
    let mut permissions = std::fs::metadata(&app_bin)
        .expect("app-bin.sh")
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&app_bin, permissions).expect("app-bin.sh runs");
    let mut command = run(std::path::Path::new("bash"));
    command
        .arg(script)
        .env("SUZATARY_REPO", code)
        .env(
            suzatary_signatures::signatures::BOX_KEY,
            suzatary_connect::cast::keys::ROOT,
        )
        .env(
            suzatary_signatures::signatures::AGENT_KEY,
            suzatary_connect::cast::keys::AGENT,
        )
        .env(
            suzatary_mail::mail::dedup_key::DEDUP_KEY,
            suzatary_connect::cast::keys::DEDUP,
        );
    command
}

/// The binary's path, for a guard's script that calls only verbs reading no deployment and sets
/// their environment itself.
pub(crate) fn path() -> &'static str {
    env!("CARGO_BIN_EXE_suzatary")
}

/// The closed world's child, `suzatary typeset`, as the box runs it: it reads its stdin and no
/// deployment.
pub(crate) fn typeset() -> suzatary_io::typeset::Closed {
    suzatary_io::typeset::Closed::new(
        env!("CARGO_BIN_EXE_suzatary"),
        suzatary_io::typeset::closed::BACKSTOP,
    )
}

/// The `HOME` every run in the running test shares, made at its first run.
fn home() -> PathBuf {
    let home = suzatary_testkit::this_test().join("home");
    std::fs::create_dir_all(home.join("run")).expect("a home for the binary's runs");
    home
}

/// [`fixture`] with the fixture box's signing seeds and a state directory under `root`, as
/// `scripts/web-e2e.sh` runs it.
#[cfg(feature = "fixture")]
pub(crate) fn keyed_fixture(root: &std::path::Path) -> Command {
    use suzatary_cli::commands::fixture::{AGENT_SEED, BOX_SEED};
    let mut command = fixture();
    command
        .env("XDG_STATE_HOME", root.join("state"))
        .env(suzatary_signatures::signatures::BOX_KEY, BOX_SEED)
        .env(suzatary_signatures::signatures::AGENT_KEY, AGENT_SEED)
        .env(
            suzatary_mail::mail::dedup_key::DEDUP_KEY,
            "e2e0000000000000000000000000000000000000000000000000000000000d3d",
        )
        .env("MALLOC_ARENA_MAX", "2");
    command
}

/// `suzatary-dev serve` over a fixture instance, ended when the test is.
#[cfg(feature = "fixture")]
pub(crate) struct Serving {
    child: std::process::Child,
    log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

#[cfg(feature = "fixture")]
impl Serving {
    /// The server over `instance`, answering once its log says it listens, with a script that
    /// says "Noted." to every turn and its mail kept under `root`.
    pub(crate) fn start(root: &std::path::Path, instance: &std::path::Path) -> Self {
        use std::io::BufRead as _;
        use std::process::Stdio;
        let script = root.join("model-script.json");
        std::fs::write(
            &script,
            r#"[{"when": "", "reply": "{\"say\": \"Noted.\"}"}]"#,
        )
        .expect("the script writes");
        let mut child = keyed_fixture(root)
            .arg("serve")
            .arg("--instance")
            .arg(instance)
            .args(["--whois-fixture", "e2e-writer@example.invalid"])
            .arg("--model-fixture")
            .arg(&script)
            .arg("--smtp-fixture")
            .arg(root.join("sent"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server starts");
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let keep = |from: Box<dyn std::io::Read + Send>| {
            let log = std::sync::Arc::clone(&log);
            let (said, heard) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(from).lines().map_while(Result::ok) {
                    if line.contains("serve: http") {
                        let _ = said.send(());
                    }
                    log.lock().expect("the log").push(line);
                }
            });
            heard
        };
        let listening = keep(Box::new(child.stdout.take().expect("piped")));
        let _ = keep(Box::new(child.stderr.take().expect("piped")));
        let serving = Serving { child, log };
        assert!(
            listening.recv().is_ok(),
            "the server ended before it listened: {}",
            serving.log()
        );
        serving
    }

    pub(crate) fn id(&self) -> u32 {
        self.child.id()
    }

    /// Every line logged until now.
    pub(crate) fn log(&self) -> String {
        self.log.lock().expect("the log").join("\n")
    }
}

#[cfg(feature = "fixture")]
impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
