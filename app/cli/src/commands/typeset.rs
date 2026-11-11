//! `suzatary typeset`: one job on stdin (`suzatary_core::template::Job`) compiled, its answer on stdout.
//!
//! The box runs this as a child of itself (`suzatary_io::typeset::Closed`). The child caps its
//! own CPU time (a looping text is stopped by the kernel) and address space (an endless
//! allocation fails here, not in the box's OOM killer), both before the text is read.

use std::io::{Read as _, Write as _};

use anyhow::Context as _;
use rustix::process::{Resource, Rlimit, setrlimit};
use suzatary_io::typeset::closed::CPU_SECONDS;

/// Plenty for one text; typst reserves stacks and arenas well under this.
const ADDRESS_SPACE: u64 = 2 << 30;

/// # Errors
/// Stdin could not be read or stdout written; a refused text is an answer, not an error.
pub fn run() -> anyhow::Result<()> {
    // The soft limit sends SIGXCPU, whose default action ends the child. The hard limit, a
    // second later, sends SIGKILL, for a child that inherited SIGXCPU ignored across `exec`.
    let cpu = Rlimit {
        current: Some(CPU_SECONDS),
        maximum: Some(CPU_SECONDS + 1),
    };
    setrlimit(Resource::Cpu, cpu).context("capping the compile's CPU time")?;
    let cap = Rlimit {
        current: Some(ADDRESS_SPACE),
        maximum: Some(ADDRESS_SPACE),
    };
    setrlimit(Resource::As, cap).context("capping the compile's address space")?;
    // Outgrowing the cap aborts the child: a refusal, not a crash to keep.
    let no_core = Rlimit {
        current: Some(0),
        maximum: Some(0),
    };
    setrlimit(Resource::Core, no_core).context("turning off the compile's core dump")?;
    let mut job = String::new();
    std::io::stdin()
        .read_to_string(&mut job)
        .context("reading the job")?;
    let answer = serde_json::from_str::<suzatary_core::template::Job>(&job)
        .map_err(|e| {
            suzatary_base::capability::Rejected::whole(format!("the job does not parse: {e}"))
        })
        .and_then(|job| suzatary_io::typeset::compile(&job));
    std::io::stdout()
        .write_all(suzatary_io::typeset::closed::print(&answer).as_bytes())
        .context("writing the answer")
}
