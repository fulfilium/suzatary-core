//! The one way a test runs the binary. A run starts from a clean environment: nothing of the
//! process that runs the suite reaches it but the tools it finds, so a suite run by a unit on the
//! box (`SUZATARY_REPO`, `SUZATARY_INSTANCE`, `SUZATARY_ROOTS`, the XDG directories) or by the
//! owner (a secret store found through `HOME`) never reads that deployment. A test sets what its
//! run reads; `binary_spawns_only_here` in `suites.rs` fails on a spawn anywhere else.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

/// What a run keeps of this process's environment: where its tools are, and where scratch goes.
const KEPT: [&str; 2] = ["PATH", "TMPDIR"];

/// `suzatary`, its environment the [`KEPT`] variables and a `HOME` and a runtime directory of
/// this process's own that start empty, as a login session or a unit has both.
pub(crate) fn suzatary() -> Command {
    run(std::path::Path::new(path()))
}

/// The executable at `exe` (a copy of the binary), run as [`suzatary`] runs the binary.
pub(crate) fn run(exe: &std::path::Path) -> Command {
    let mut command = Command::new(exe);
    command
        .env_clear()
        .env("HOME", home())
        .env("XDG_RUNTIME_DIR", home().join("run"));
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
            suzatary_core::cast::keys::ROOT,
        )
        .env(
            suzatary_signatures::signatures::AGENT_KEY,
            suzatary_core::cast::keys::AGENT,
        )
        .env(
            suzatary_core::mail::dedup_key::DEDUP_KEY,
            suzatary_core::cast::keys::DEDUP,
        );
    command
}

/// The binary's path, for a script a guard runs that calls only a verb reading no deployment
/// (`public-tree.sh`'s matcher, `suzatary leaks`, over the lists it is handed): the script sets
/// that run's environment, as a deploy's does.
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

/// The `HOME` every run of this process shares, made empty once.
fn home() -> &'static PathBuf {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let home = std::env::temp_dir().join(format!("suzatary-tests-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("run")).expect("a home for the binary's runs");
        home
    })
}
