//! A compile bounded: each one is a child process (`suzatary typeset`) that caps its own CPU
//! time and address space before it reads the text, since a parent's `pre_exec` is `unsafe` and
//! every crate here forbids it.

use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;

use rustix::process::Signal;
use suzatary_base::capability::{Rejected, Typeset};
use suzatary_core::template::Job;

use super::Compiler;
use crate::run::{Cmd, RunError, succeed};

/// The CPU time one compile may use, counted from the child's start and across its threads.
/// A todo's text uses 15 to 35 ms.
pub const CPU_SECONDS: u64 = 2;

/// The wall-clock limit, for a child that blocks without using CPU; long enough that a busy box
/// never reaches it.
pub const BACKSTOP: Duration = Duration::from_mins(1);

/// A document longer than this is not a page.
const MAX_ANSWER: usize = 32 << 20;

/// Typst, one child process per compile.
#[derive(Debug, Clone)]
pub struct Closed {
    exe: PathBuf,
    backstop: Duration,
}

impl Closed {
    /// `exe typeset` is the child, killed after `backstop` of wall-clock time.
    #[must_use]
    pub fn new(exe: impl Into<PathBuf>, backstop: Duration) -> Self {
        Self {
            exe: exe.into(),
            backstop,
        }
    }

    /// This binary as the child, or `suzatary` on `PATH` when its path is unknown.
    #[must_use]
    pub fn this_binary() -> Self {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("suzatary"));
        Self::new(exe, BACKSTOP)
    }
}

impl Typeset for Closed {
    fn closed(&self, source: &str) -> Result<String, Rejected> {
        self.compile(&Job::text(source))
    }
}

impl Compiler for Closed {
    fn compile(&self, job: &Job) -> Result<String, Rejected> {
        let job = serde_json::to_vec(job)
            .map_err(|e| Rejected::whole(format!("the job does not print as JSON: {e}")))?;
        let cmd = Cmd::new(&self.exe)
            .arg("typeset")
            .stdin(job)
            .timeout(self.backstop)
            .max_stdout(MAX_ANSWER);
        match succeed(&cmd) {
            Ok(ran) => read(&ran.out()),
            Err(RunError::Exited { status, .. }) if out_of_cpu(status) => Err(Rejected::whole(
                format!("the text did not finish compiling within {CPU_SECONDS} s of CPU time"),
            )),
            Err(RunError::Exited { stderr, .. }) => {
                Err(Rejected::whole(format!("the compile stopped: {stderr}")))
            }
            Err(RunError::TimedOut { after, .. }) => Err(Rejected::whole(format!(
                "the compile did not answer within {after:?}"
            ))),
            Err(failed) => Err(Rejected::whole(failed.to_string())),
        }
    }
}

/// Whether the kernel stopped the child at its CPU limit: `SIGXCPU` at the soft limit, or
/// `SIGKILL` at the hard limit a second later, where the child inherited `SIGXCPU` ignored.
fn out_of_cpu(status: ExitStatus) -> bool {
    status
        .signal()
        .is_some_and(|signal| signal == Signal::XCPU.as_raw() || signal == Signal::KILL.as_raw())
}

/// What the child prints: the compile's answer as JSON, on one line.
#[must_use]
pub fn print(answer: &Result<String, Rejected>) -> String {
    serde_json::to_string(answer)
        .unwrap_or_else(|e| format!(r#"{{"Err":{{"diagnostics":[{{"message":"{e}"}}]}}}}"#))
}

/// The child's answer read back; anything else is a refusal.
pub fn read(printed: &str) -> Result<String, Rejected> {
    serde_json::from_str::<Result<String, Rejected>>(printed).unwrap_or_else(|e| {
        Err(Rejected::whole(format!(
            "the compile answered an unexpected shape: {e}"
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_that_cannot_start_is_a_refusal() {
        let closed = Closed::new("/nonexistent/suzatary", BACKSTOP);
        assert!(closed.closed("x").is_err());
    }

    /// Only the two signals the CPU limit sends read as the CPU limit: an abort (the address
    /// space cap) or an exit is the compile stopping.
    #[test]
    fn only_the_cpu_limits_signals_are_out_of_cpu() {
        let killed_by = |signal: Signal| ExitStatus::from_raw(signal.as_raw());
        assert!(out_of_cpu(killed_by(Signal::XCPU)));
        assert!(out_of_cpu(killed_by(Signal::KILL)));
        assert!(!out_of_cpu(killed_by(Signal::ABORT)));
        assert!(!out_of_cpu(ExitStatus::from_raw(1 << 8)));
    }
}
