//! The deployed binary: the instance flag, then a [`Verb`].

#![forbid(unsafe_code)]

use clap::{CommandFactory, FromArgMatches, Parser};
use suzatary_cli::instance::Instance;
use suzatary_cli::verb::{Verb, exit_on};

/// Suzatary: FPL-driven todos and mail triage, over the Prodrome.
#[derive(Debug, Parser)]
#[command(name = "suzatary", about, long_about = None)]
struct Cli {
    /// The instance file; defaults to `SUZATARY_INSTANCE`, then `instance.toml` here.
    #[arg(long, global = true, value_name = "PATH")]
    instance: Option<std::path::PathBuf>,
    /// What to do.
    #[command(subcommand)]
    verb: Verb,
}

fn main() {
    suzatary_cli::verb::logs_when_asked();
    // The version is the installed release's ([`suzatary_cli::stamp`]), not the crate's.
    let matches = Cli::command()
        .version(suzatary_cli::stamp::version())
        .get_matches();
    suzatary_cli::verb::tell_the_journal(&matches);
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    exit_on(
        "suzatary",
        cli.verb.run(&Instance::locate(cli.instance.as_deref())),
    );
}
