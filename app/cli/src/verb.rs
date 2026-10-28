//! The argument tree both binaries share: every verb the deployed program
//! answers, and the one `match` that runs it.

use std::path::Path;

use clap::Subcommand;

use crate::commands::{
    coding, decision, device, doctor, heartbeat, init, instance, leaks, mail, mcp, notify, plugin,
    ran, release, replica, secretary, secretspec, serve, site, sources, todo, todos, typeset, up,
    vault, verify, weather,
};

/// Every verb the deployed binary answers.
#[derive(Debug, Subcommand)]
pub enum Verb {
    /// Report which capabilities this binary can perform, and what it reads this deployment to be.
    Doctor(doctor::Args),
    /// Write `instance.toml` from flags, prompting for what is missing at a terminal.
    Init(init::Args),
    /// One key of `instance.toml`, read or written by the parser the box reads it with.
    Instance(instance::Args),
    /// Serve the web app over the chain (private; writes need a named peer).
    Serve(serve::Args),
    /// Push a notice to the phone (a saved briefing, a conversation's brief, or `no-brief`
    /// when the brief failed); it carries no text.
    Notify(notify::Args),
    /// Serve the triage agent's tools over stdio (MCP); the capability bundle is the fence.
    Mcp,
    /// Author, read and record todos on the chain (trusted).
    Todo(todo::Args),
    /// Generate the static attestable-history site from the chain.
    Web(site::Args),
    /// Read the todo list off the chain.
    Todos(todos::Args),
    /// Record, revise, withdraw, list and view the recorded decisions (designs and plans).
    Decision(decision::Args),
    /// The version the project chain reads, and the release a train cuts from it.
    Release(release::Args),
    /// Who may write: the box's signing keys, and the device keys trusted or revoked.
    Device(device::Args),
    /// Pull every account, reindex, re-tag the inbox, and keep the report in the state directory.
    Sync,
    /// The triage queue: inbox mail not yet seen by hash, and not noise.
    New(mail::NewArgs),
    /// Whether this pass has anything for the model; exits 3 when nothing is new and all synced.
    Gate,
    /// Record how a triage pass ended, where the brief's alarm reads it.
    Heartbeat(heartbeat::Args),
    /// A model run's journal line: its counts and ids, never its words.
    Ran(ran::Args),
    /// Run any notmuch query, newest first.
    Search(mail::SearchArgs),
    /// A full message by notmuch id, or by canonical hash or a unique prefix of one.
    Show(mail::ShowArgs),
    /// Record a decision about a canonical message.
    Mark(mail::MarkArgs),
    /// Manage the wholesale-ignore list.
    Noise(mail::NoiseArgs),
    /// A read-only pull of a whole server folder into its own maildir leaf, kept out of `sync`.
    Backfill(mail::BackfillArgs),
    /// Send mail; human-only, since the agent's capability bundle has nothing that sends.
    Send(mail::SendArgs),
    /// Authorise an account with the device-code flow, or with no address set up this box.
    Auth(mail::AuthArgs),
    /// Long-running mail verbs: `idle` wakes triage on new mail ahead of the timer.
    Mail(mail::MailArgs),
    /// Encrypted state snapshot to Proton Drive (human-only).
    Vault(vault::Args),
    /// Mail replica via muchsync — run these ON the replica machine.
    Replica(replica::Args),
    /// Enable the fetcher's services and run the doctor; refuses on a box not named in `<data>/FETCHER`.
    Up,
    /// Talk to the secretary: open a briefing conversation, list them, read one.
    Secretary(secretary::Args),
    /// One line of weather for the briefing, for `[persona]`'s city.
    Weather,
    /// Poll cloud coding sessions against their `claude/` branches; starting one is the app's tap.
    Coding(coding::Args),
    /// Run the `[[plugins]]` sources (a calendar's links) into the cache tier; the box's timer.
    Plugin(plugin::Args),
    /// The sources the box reads from: their health register's migration.
    Sources(sources::Args),
    /// The leak guard's matcher: each line of a tree to publish that names this deployment.
    Leaks(leaks::Args),
    /// Check the store: every object rehashed, every parent present, every head reachable.
    Verify,
    /// Print the file `secretspec set <NAME> --file` takes: every secret name this deployment
    /// may set, the code root's own and the ones its tables derive.
    Secretspec,
    /// Compile the text on stdin in the closed world (the child the box bounds in time).
    #[command(hide = true)]
    Typeset,
}

impl Verb {
    /// Do it, against the instance file at `path`.
    ///
    /// # Errors
    /// The verb's own.
    pub fn run(self, path: &Path) -> anyhow::Result<()> {
        match self {
            Verb::Doctor(args) => doctor::run(path, &args),
            Verb::Init(args) => init::run(path, args),
            Verb::Instance(args) => instance::run(path, &args),
            Verb::Serve(args) => serve::run(path, args),
            Verb::Notify(args) => notify::run(path, &args),
            Verb::Mcp => mcp::run(path),
            Verb::Todo(args) => todo::run(path, args),
            Verb::Web(args) => site::run(path, &args),
            Verb::Todos(args) => todos::run(path, &args),
            Verb::Decision(args) => decision::run(path, &args),
            Verb::Release(args) => release::run(path, &args),
            Verb::Device(args) => device::run(path, &args),
            Verb::Sync => mail::sync(path),
            Verb::New(args) => mail::queue(path, &args),
            Verb::Gate => mail::gate(path),
            Verb::Heartbeat(args) => heartbeat::run(path, &args),
            Verb::Ran(args) => ran::run(&args),
            Verb::Search(args) => mail::search(path, &args),
            Verb::Show(args) => mail::show(path, &args),
            Verb::Mark(args) => mail::mark(path, &args),
            Verb::Noise(args) => mail::noise(path, &args),
            Verb::Backfill(args) => mail::backfill(path, &args),
            Verb::Send(args) => mail::send(path, &args),
            Verb::Auth(args) => mail::auth(path, &args),
            Verb::Mail(args) => mail::mail_command(path, &args),
            Verb::Vault(args) => vault::run(path, &args),
            Verb::Replica(args) => replica::run(&args),
            Verb::Up => up::run(path),
            Verb::Secretary(args) => secretary::run(path, &args),
            Verb::Weather => weather::run(path),
            Verb::Coding(args) => coding::run(path, &args),
            Verb::Plugin(args) => plugin::run(path, &args),
            Verb::Sources(args) => sources::run(path, &args),
            Verb::Leaks(args) => leaks::run(&args),
            Verb::Verify => verify::run(path),
            Verb::Secretspec => secretspec::run(path),
            Verb::Typeset => typeset::run(),
        }
    }
}

/// Print a failure and its causes under the program's name, and exit 1.
pub fn exit_on(program: &str, result: anyhow::Result<()>) {
    if let Err(failed) = result {
        for (depth, cause) in failed.chain().enumerate() {
            if depth == 0 {
                eprintln!("{program}: {cause}");
            } else {
                eprintln!("  {cause}");
            }
        }
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Verb;

    /// The argument tree alone, as the binaries parse it.
    #[derive(Debug, Parser)]
    struct Tree {
        #[command(subcommand)]
        verb: Verb,
    }

    /// The one-time migrations that have run on the box are no verbs any more: each is refused as
    /// an unknown subcommand, as is its `--check` or `--dry` form.
    #[test]
    fn the_migrations_that_ran_are_no_verbs() {
        for argv in [
            &["secretary", "bound-answers"][..],
            &["secretary", "bound-answers", "--dry"],
            &["secretary", "carry-memory"],
            &["secretary", "carry-memory", "--check"],
            &["secretary", "migrate-inbox"],
            &["secretary", "migrate-inbox", "--dry"],
            &["secretary", "migrate-receipts"],
            &["secretary", "migrate-receipts", "--dry"],
            &["mail", "migrate-decisions"],
            &["mail", "migrate-decisions", "--dry"],
            &["mail", "migrate-health"],
            &["mail", "migrate-health", "--check"],
            &["plugin", "migrate-ledgers"],
            &["plugin", "migrate-ledgers", "--check"],
            &["plugin", "migrate"],
        ] {
            let refused =
                Tree::try_parse_from(std::iter::once("suzatary").chain(argv.iter().copied()))
                    .expect_err("no such verb");
            assert_eq!(
                refused.kind(),
                clap::error::ErrorKind::InvalidSubcommand,
                "{argv:?}: {refused}"
            );
        }
    }
}
