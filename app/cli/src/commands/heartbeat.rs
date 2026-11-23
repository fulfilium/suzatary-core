//! `suzatary heartbeat`: how a triage pass ended, recorded at the instance's `mail.heartbeat`,
//! the path the brief's alarm reads, so the wrapper never spells it.

use std::path::Path;

use clap::Subcommand;
use suzatary_base::capability::Clock as _;
use suzatary_io_local::pulse::Pulse;
use suzatary_triage::mail::heartbeat::{Pass, Stage};

use crate::instance::Instance;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// How the pass ended.
    #[command(subcommand)]
    ended: Ended,
}

#[derive(Debug, Subcommand)]
enum Ended {
    /// Every stage did its work: the heartbeat moves.
    Ok,
    /// A stage failed: the run of failures grows, and the heartbeat stays where it was.
    Failed {
        /// The first stage that failed: sync, triage, review, sweep, verify, push or record.
        stage: Stage,
    },
}

pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let pass = match args.ended {
        Ended::Ok => Pass::Ok,
        Ended::Failed { stage } => Pass::Failed(stage),
    };
    let now = prodrome_core::fpl::instant_of(suzatary_io_local::clock::SystemClock.now());
    let outcome = Pulse::at(&instance.mail.heartbeat).record(pass, now)?;
    println!("{}", outcome.line());
    Ok(())
}
