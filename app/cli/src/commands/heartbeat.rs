//! `suzatary heartbeat`: how a triage pass ended, recorded at the instance's `mail.heartbeat`,
//! the path the brief's alarm reads, so the wrapper never spells it.

use std::path::Path;

use clap::Subcommand;
use suzatary_base::capability::Clock as _;
use suzatary_core::mail::heartbeat::{Pass, Stage};
use suzatary_io::pulse::Pulse;

use crate::instance::Instance;

/// What `suzatary heartbeat` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// How the pass ended.
    #[command(subcommand)]
    ended: Ended,
}

/// How a pass ended.
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

/// `suzatary heartbeat ok | failed <stage>`: record the pass and print the outcome's line.
///
/// # Errors
/// The instance, or a file that cannot be read or written.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let pass = match args.ended {
        Ended::Ok => Pass::Ok,
        Ended::Failed { stage } => Pass::Failed(stage),
    };
    let now = prodrome_core::fpl::instant_of(suzatary_io::clock::SystemClock.now());
    let outcome = Pulse::at(&instance.mail.heartbeat).record(pass, now)?;
    println!("{}", outcome.line());
    Ok(())
}
