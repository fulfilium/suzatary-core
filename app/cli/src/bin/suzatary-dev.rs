//! The development binary: every verb of the deployed one, plus the end-to-end fixture and the
//! guide's assembly, which the deployed one does not contain. It exists only under the `fixture` feature.

#![forbid(unsafe_code)]

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use suzatary_cli::commands::{fixture, fixture_live, guide};
use suzatary_cli::instance::Instance;
use suzatary_cli::verb::{Verb, exit_on};

/// Suzatary's development binary: every verb of `suzatary`, plus the end-to-end fixture.
/// Never deployed.
#[derive(Debug, Parser)]
#[command(name = "suzatary-dev", long_about = None)]
struct Cli {
    /// The instance file; defaults to `SUZATARY_INSTANCE`, then `instance.toml` here.
    #[arg(long, global = true, value_name = "PATH")]
    instance: Option<std::path::PathBuf>,
    /// What to do.
    #[command(subcommand)]
    verb: Dev,
}

#[derive(Debug, Subcommand)]
enum Dev {
    /// Build the end-to-end fixture's chain and instance file.
    FixtureChain(fixture::Args),
    /// Copy a deployment's data tree for a server the browser suite reads it through.
    FixtureLive(fixture_live::Args),
    /// Run the agent's writer once against a fixture deployment.
    FixtureTriage(fixture::TriageArgs),
    /// Serve the fake Google the browser suite signs an account in with.
    FakeGoogle(fixture::FakeGoogleArgs),
    /// Assemble the guide around the cast's deployment: its HTML and its PDF's source.
    Guide(guide::Args),
    #[command(flatten)]
    App(Box<Verb>),
}

fn main() {
    // The version is the installed release's ([`suzatary_cli::stamp`]), not the crate's.
    let cli = Cli::from_arg_matches(
        &Cli::command()
            .version(suzatary_cli::stamp::version())
            .get_matches(),
    )
    .unwrap_or_else(|e| e.exit());
    let path = Instance::locate(cli.instance.as_deref());
    exit_on(
        "suzatary-dev",
        match cli.verb {
            Dev::FixtureChain(args) => fixture::run(&args),
            Dev::FixtureLive(args) => fixture_live::run(&args),
            Dev::FixtureTriage(args) => fixture::triage(&args),
            Dev::FakeGoogle(args) => fixture::fake_google(&args),
            Dev::Guide(args) => guide::run(&args),
            Dev::App(verb) => verb.run(&path),
        },
    );
}
