//! `suzatary doctor`: what this binary can do, and what it reads this deployment to be.
//!
//! It prints what it read, including grants and sync strategies, rather than merely
//! parsing it, because a declaration says nothing about what is actually deployed.

use std::path::Path;

use suzatary_core::capability::Clock;
use suzatary_core::mail::account::Sync;
use suzatary_io::clock::SystemClock;

use crate::instance;

/// What `suzatary doctor` was told; the default reads the deployment.
#[derive(Debug, Default, clap::Args)]
pub struct Args {
    /// Print what leaves this box, as `AGENT.md` holds it, and read no deployment.
    #[arg(long)]
    outbound: bool,
}

/// One capability of the application, and whether this binary has an interpreter for it.
struct Capability {
    name: &'static str,
    stage: &'static str,
    interpreter: Option<&'static str>,
}

const CAPABILITIES: &[Capability] = &[
    Capability {
        name: "Clock",
        stage: "0 — the skeleton",
        interpreter: Some("suzatary_io::clock::SystemClock"),
    },
    Capability {
        name: "Whois",
        stage: "2 — the server",
        interpreter: None,
    },
    Capability {
        name: "Notify",
        stage: "2 — the server",
        interpreter: None,
    },
    Capability {
        name: "Render",
        stage: "2 — the server",
        interpreter: None,
    },
    Capability {
        name: "Mailbox",
        stage: "6 — mail",
        interpreter: Some("suzatary_io::mailbox::Notmuch"),
    },
    Capability {
        name: "Corpus",
        stage: "6 — mail",
        interpreter: Some("suzatary_io::corpus::Operator"),
    },
    Capability {
        name: "Repository",
        stage: "6 — mail",
        interpreter: Some("suzatary_io::repository::Checkout"),
    },
    Capability {
        name: "Manuals",
        stage: "6 — mail",
        interpreter: Some("suzatary_io::manuals::Handbook"),
    },
];

/// The version, the clock, and which capability has an interpreter.
fn capabilities() {
    let clock = SystemClock;
    println!(
        "suzatary {} — {}",
        crate::stamp::version(),
        clock.now().isoformat()
    );
    println!("\ncapabilities");
    for capability in CAPABILITIES {
        match capability.interpreter {
            Some(interpreter) => println!("  {:<8} yes  {interpreter}", capability.name),
            None => println!("  {:<8} no   stage {}", capability.name, capability.stage),
        }
    }
    println!("\nverbs — all of them, in this binary");
    let tree = <crate::verb::Verb as clap::Subcommand>::augment_subcommands(clap::Command::new(
        "suzatary",
    ));
    let verbs: Vec<&str> = tree
        .get_subcommands()
        .map(clap::Command::get_name)
        .collect();
    for chunk in verbs.chunks(6) {
        println!("  {}", chunk.join("  "));
    }
}

/// The person this deployment serves, printed so a silently defaulted name shows up.
fn print_persona(persona: &suzatary_core::persona::Persona) {
    println!(
        "  persona   {} — {} ({})",
        persona.name,
        persona.city,
        persona.zone.name()
    );
}

/// The `config` line's words.
fn notmuch_config_line(config: Option<&Path>) -> String {
    config.map_or_else(
        || "none — mail search refuses (no ambient fallback)".to_owned(),
        |named| named.display().to_string(),
    )
}

/// What this binary reads the deployment to be.
// One line per fact is the shape `doctor` exists for, so this is not split into helpers.
#[allow(clippy::too_many_lines)]
fn deployment(path: &Path) {
    match instance::Instance::open(path) {
        Err(refused) => {
            println!("\ninstance  {}", path.display());
            for cause in refused.chain() {
                println!("  ✗ {cause}");
            }
        }
        Ok(read) => {
            println!("\ninstance  {}", read.path.display());
            println!("  repo      {}", read.code().display());
            // In file order: the first chain is what bare paths and unflagged verbs mean.
            for (index, chain) in read.chains.iter().enumerate() {
                let mark = if index == 0 { "*" } else { " " };
                println!(
                    "  chain   {mark} {:<10} {}",
                    chain.policy.name,
                    chain.events.display()
                );
                println!(
                    "              untrusted {}",
                    chain.policy.untrusted_names().join(", ")
                );
                println!(
                    "              actors    {}",
                    chain
                        .policy
                        .roster
                        .iter()
                        .map(prodrome_core::event::Actor::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            println!("  (* is the first chain: the bare /api paths and an unflagged verb)");
            println!("  dist      {}", read.serving.dist.display());
            println!("  briefings {}", read.serving.briefings.display());
            println!("  tls       {}", read.serving.tls_dir.display());
            println!("  bind      {}:{}", read.serving.host, read.serving.port);
            println!("  app       {}", read.deployment.app_url);
            println!("  door      {}", door_line(&read));
            println!("  signs     {}", signs(&read));
            print_persona(&read.persona);
            println!(
                "  viewers   {}",
                read.deployment
                    .viewers
                    .iter()
                    .map(suzatary_core::capability::Login::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for (login, actor) in &read.deployment.actors {
                println!("  writes as {login} → {}", actor.as_str());
            }
            println!(
                "  mail      {} ({} accounts, state {})",
                read.mail.root.display(),
                read.mail.accounts.accounts().len(),
                read.mail.state.display()
            );
            for account in read.mail.accounts.accounts() {
                let how = match &account.sync {
                    Sync::Gmail => "gmail (lieer)".to_owned(),
                    Sync::Google => {
                        "google (lieer, under the box's sign-in with Google)".to_owned()
                    }
                    Sync::MsImap(ms) => format!("ms-imap ({})", ms.authority),
                    Sync::Proton(proton) => format!("proton ({})", proton.channel),
                };
                let folders = if account.inbox_folders().is_empty() {
                    "labels".to_owned()
                } else {
                    account.inbox_folders().join(", ")
                };
                println!(
                    "    {:<28} {:<20} {how}  [{folders}]",
                    account.email, account.subdir
                );
            }
            println!("  notmuch   {}", read.mail.notmuch.display());
            let config = notmuch_config_line(read.mail.notmuch_config.as_deref());
            println!("  config    {config}");
            println!("  mbsyncrc  {}", read.mail.mbsyncrc.display());
            println!("  last sync {}", read.paths.last_sync.display());
            println!("  grants    {}", read.paths.oauth.display());
            for (email, held) in super::mail::grants(&read) {
                let says = if held.is_empty() {
                    "none — run `suzatary auth <email>` at a terminal".to_owned()
                } else {
                    held.join(", ")
                };
                println!("    {email:<28} {says}");
            }
            let watched = super::mail::idle_watches(&read);
            if watched.is_empty() {
                println!("  idle      no IMAP accounts — nothing to watch");
            } else {
                println!(
                    "  idle      {} (gmail and proton stay on the 5-minute timer)",
                    watched.join(", ")
                );
            }
            print_models(&read);
        }
    }
}

/// The deployment's root (SPEC law 42), and whether its two keys resolve: names, never a
/// value.
fn signs(read: &instance::Instance) -> String {
    use suzatary_core::signatures::Signers;
    let root = read.root().as_str();
    match read.signers() {
        Signers::Held(_) => format!("root {root}; the box's and the agent's keys resolve"),
        Signers::Missing(why) => format!("root {root}; ✗ {why}"),
    }
}

/// A secret's state as `doctor` prints it: a locked store is never printed as missing.
fn said(state: suzatary_core::capability::SecretState) -> &'static str {
    match state {
        suzatary_core::capability::SecretState::Set => "resolves",
        suzatary_core::capability::SecretState::Missing => "MISSING",
        suzatary_core::capability::SecretState::Locked => "LOCKED (the store could not answer)",
        suzatary_core::capability::SecretState::Failed => "FAILED (the store could not be run)",
    }
}

/// The Claude models `[secretary]` names, the compactor's reserve, and whether the token the
/// `claude` CLI reads resolves.
fn print_models(read: &instance::Instance) {
    println!("\nmodels");
    println!(
        "  chat, reviews and titles  {}",
        suzatary_io::model::name_of(read.secretary.model.as_str())
    );
    match read.secretary.compacting() {
        suzatary_core::secretary::compacting::Compacting::Off => {
            println!("  compactor                 off");
        }
        suzatary_core::secretary::compacting::Compacting::On { model, .. } => println!(
            "  compactor                 {}, leaving {:.0}% of the subscription",
            suzatary_io::model::name_of(model.as_str()),
            read.secretary.reserve.get() * 100.0
        ),
    }
    let secrets = read.secrets();
    match suzatary_io::secrets::state(&secrets.get(
        suzatary_io::model::TOKEN,
        "doctor: checking whether the model's token resolves",
    )) {
        suzatary_core::capability::SecretState::Missing => println!(
            "  the model's token is not set: run {}, or connect the model from the app",
            read.reach().secret(suzatary_io::model::TOKEN)
        ),
        state => println!("  token     {}", said(state)),
    }
}

/// `suzatary doctor`: the program, then the deployment.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    if args.outbound {
        print!("{}", super::outbound::markdown());
        return Ok(());
    }
    capabilities();
    deployment(path);
    Ok(())
}

/// Where the gate answers, against the port `serve` would bind: the unit's `SUZATARY_WEB_PORT`
/// (its `--port`) where it runs under the unit, else what the unit will bind.
fn door_line(read: &crate::instance::Instance) -> String {
    let under = std::env::var("SUZATARY_WEB_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok());
    door_said(
        crate::commands::serve::door(&read.deployment.app_url),
        under.map_or(Binds::Shell(read.serving.port), Binds::Unit),
    )
}

/// Which port `serve` binds, as `doctor` can know it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Binds {
    /// Run by the unit: the port it was handed.
    Unit(u16),
    /// Run in a shell: the instance file's port, which a hand-run `serve` binds, while the unit
    /// binds `services.suzatary.web.port`.
    Shell(u16),
}

/// The door line: a mismatch is an alarm only where the port compared is the one `serve` will
/// bind; in a shell it says what the unit binds and what a hand-run `serve` would.
fn door_said(door: Option<suzatary_core::gate::Door>, binds: Binds) -> String {
    match (door, binds) {
        (None, _) => "none: the gate cannot read app_url, and serve will not start".to_owned(),
        (Some(door), Binds::Unit(port) | Binds::Shell(port)) if door.port() == port => {
            door.to_string()
        }
        (Some(door), Binds::Unit(port)) => format!(
            "{door}, but the unit binds port {port}: it will not start until \
             services.suzatary.web.port is {}",
            door.port()
        ),
        (Some(door), Binds::Shell(port)) => format!(
            "{door}; the unit binds services.suzatary.web.port (443 unless the system \
             configuration sets another), and a `serve` run by hand here binds {port} and would \
             refuse this door"
        ),
    }
}
