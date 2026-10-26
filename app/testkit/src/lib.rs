//! Programs a test makes and then runs.
//!
//! Linux refuses to `execve` a file that any process holds open for writing (`ETXTBSY`). When a
//! test thread writes a program, a `fork` on another thread copies the open descriptor into a
//! child, which keeps it until its own `exec`; a run of the program in that moment fails. So a
//! test's process never runs a file that it opened for writing:
//!
//! - [`fake_program`] writes the program's text to a data file and links the program's name to
//!   the committed wrapper `fake-program`, which no test opens for writing.
//! - [`write_program`] and [`copy_program`] are for a test that needs a regular file at the
//!   path, with its own bytes. A child process writes the file, so the test's process never
//!   holds a descriptor on it.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The committed wrapper: it runs `<link>.sh` with the arguments and environment it was given.
const WRAPPER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fake-program");

/// A program at `<dir>/<name>`, `dir` created if missing, that runs `body` with its arguments and environment, under the
/// interpreter that `body`'s `#!` line names, or under `sh`. Inside `body`, `$0` is
/// `<dir>/<name>.sh`. A second call with the same name replaces the body.
///
/// # Panics
///
/// When `dir` cannot be written, or `<dir>/<name>` exists and is not a link to the wrapper.
#[must_use]
pub fn fake_program(dir: &Path, name: &str, body: &str) -> PathBuf {
    let program = dir.join(name);
    std::fs::create_dir_all(dir).expect("the program's directory");
    std::fs::write(dir.join(format!("{name}.sh")), body).expect("the program's text writes");
    if let Err(e) = std::os::unix::fs::symlink(WRAPPER, &program) {
        assert!(
            e.kind() == std::io::ErrorKind::AlreadyExists
                && std::fs::read_link(&program).is_ok_and(|to| to == Path::new(WRAPPER)),
            "{}: {e}",
            program.display()
        );
    }
    program
}

/// A regular executable file at `path` that holds `bytes`, written by a child process.
///
/// # Panics
///
/// When the child cannot write the file.
pub fn write_program(path: &Path, bytes: &[u8]) {
    let mut child = by_child("cat >\"$1\" && chmod 755 \"$1\"", &[path])
        .stdin(Stdio::piped())
        .spawn()
        .expect("sh starts");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(bytes)
        .expect("the bytes reach the child");
    let status = child.wait().expect("the child ends");
    assert!(status.success(), "{} was not written", path.display());
}

/// A regular executable file at `to` that holds the bytes of `from`, copied by a child process.
///
/// # Panics
///
/// When the child cannot copy the file.
pub fn copy_program(from: &Path, to: &Path) {
    let status = by_child("cp \"$1\" \"$2\" && chmod 755 \"$2\"", &[from, to])
        .status()
        .expect("sh starts");
    assert!(status.success(), "{} was not copied", to.display());
}

/// `sh` running `script` with `args` as its positional parameters.
fn by_child(script: &str, args: &[&Path]) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script, "sh"]).args(args);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Many threads make and run programs while other threads fork without pause; no run is
    /// refused.
    #[test]
    fn fake_programs_run_while_other_threads_fork() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    while !done.load(Ordering::Relaxed) {
                        Command::new("/bin/sh")
                            .args(["-c", ":"])
                            .status()
                            .expect("sh runs");
                    }
                });
            }
            let runners: Vec<_> = (0..8)
                .map(|thread| {
                    let dir = dir.path();
                    scope.spawn(move || {
                        for n in 0..40 {
                            let name = format!("p{thread}-{n}");
                            let program = fake_program(dir, &name, "#!/bin/sh\necho \"$1\"\n");
                            let ran = Command::new(&program).arg(&name).output();
                            assert!(
                                matches!(&ran, Ok(out) if out.stdout == format!("{name}\n").as_bytes()),
                                "{name}: {ran:?}"
                            );
                        }
                    })
                })
                .collect();
            let ran: Vec<_> = runners
                .into_iter()
                .map(std::thread::ScopedJoinHandle::join)
                .collect();
            done.store(true, Ordering::Relaxed);
            for runner in ran {
                runner.expect("a runner ends");
            }
        });
    }

    /// An open write descriptor on a program refuses its run; one on a fake program's text
    /// does not, and the programs [`write_program`] and [`copy_program`] made hold none.
    #[test]
    fn only_a_descriptor_on_the_executed_file_refuses_a_run() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let script = "#!/bin/sh\nexit 0\n";

        let old = dir.path().join("old");
        let mut held = std::fs::File::create(&old).expect("created");
        held.write_all(script.as_bytes()).expect("written");
        std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let refused = Command::new(&old).status().expect_err("a busy file");
        assert_eq!(refused.kind(), ErrorKind::ExecutableFileBusy);
        drop(held);

        let fake = fake_program(dir.path(), "fake", script);
        let _held = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("fake.sh"))
            .expect("the text opens for writing");
        assert!(Command::new(&fake).status().expect("it runs").success());

        let written = dir.path().join("written");
        write_program(&written, script.as_bytes());
        assert!(Command::new(&written).status().expect("it runs").success());
        let copied = dir.path().join("copied");
        copy_program(&written, &copied);
        assert!(Command::new(&copied).status().expect("it runs").success());
    }
}
