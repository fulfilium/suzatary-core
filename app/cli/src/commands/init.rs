//! `suzatary init`: ask who the deployment serves, write `instance.toml`, begin the chains, and
//! print what only the person can do next.
//!
//! The answers are one [`Onboarding`], parsed once here: flags first, a missing one asked for
//! at a terminal and refused by name off one, the rule the device-code flow keeps. Nothing
//! asked is a secret, so no secret is ever an argument; the steps say where each one is typed.
//! An existing file is refused unless `--force`.

mod onboarding;
pub mod options;

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use clap::{Parser as _, ValueEnum as _};
use prodrome_core::event::Actor;
use suzatary_core::capability::Login;
use suzatary_core::zone::Zone;

use crate::instance::Instance;

pub use onboarding::{
    BoxName, Door, FirstAccount, Onboarding, Person, Provider, Step, model_steps,
};
use suzatary_core::connect::Reach;

/// What `suzatary init` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Your name, as the secretary addresses you (`[persona].name`).
    #[arg(long)]
    name: Option<String>,
    /// The time zone you live in, by name (`America/New_York`); yours, so change it when you
    /// move.
    #[arg(long)]
    zone: Option<String>,
    /// The city `suzatary weather` forecasts; none when absent.
    #[arg(long, default_value = "")]
    city: String,
    /// The actor your own writes carry; your first name in lower case when absent.
    #[arg(long)]
    actor: Option<String>,
    /// This box's `MagicDNS` name, `Self.DNSName` in `tailscale status --json`: the app answers
    /// at `https://<it>`.
    #[arg(long = "box", value_name = "NAME")]
    box_name: Option<String>,
    /// A Tailscale login of yours: it opens the app and writes as you. Repeatable, at least one.
    #[arg(long = "login", value_name = "LOGIN")]
    logins: Vec<String>,
    /// A category of todos, most important first. Repeatable; work, study, project,
    /// home and other when absent.
    #[arg(long = "category", value_name = "NAME")]
    categories: Vec<String>,
    /// Your first mail account's address; more are added to the file by hand.
    #[arg(long)]
    email: Option<String>,
    /// Who serves that account.
    #[arg(long, value_enum)]
    mail_provider: Option<Provider>,
    /// The code root, relative to the instance file.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    /// The data root (the chains, `state.json`, the conversations), relative to the instance
    /// file.
    #[arg(long, default_value = ".")]
    data: PathBuf,
    /// Overwrite an existing instance file. Without it, an existing file is refused untouched.
    #[arg(long)]
    force: bool,
    /// Print the choices this verb offers, as `AGENT.md` holds them, and write nothing.
    #[arg(long, exclusive = true)]
    options: bool,
}

/// `suzatary init`: keep or mint the box's keys in the secret store, write `instance.toml`
/// declaring their root, begin the chains, and say what is left.
///
/// # Errors
/// As [`run_keyed`].
pub fn run(path: &Path, args: Args) -> anyhow::Result<()> {
    // Read before anything is minted or written, so an environment no box path fits refuses
    // with nothing done.
    run_keyed(
        path,
        args,
        &crate::paths::Env::read()?,
        super::device::kept_or_minted_in,
    )
}

/// [`run`] in `env`, the box's keys taken from `keys` over the secret store the code root
/// declares.
///
/// # Errors
/// An existing file without `--force`, an answer refused, the keys not kept or minted, or the
/// file not written or not reading back.
pub fn run_keyed(
    path: &Path,
    args: Args,
    env: &crate::paths::Env,
    keys: impl FnOnce(
        &suzatary_io::secrets::Secretspec,
    ) -> anyhow::Result<suzatary_core::signatures::BoxKeys>,
) -> anyhow::Result<()> {
    if args.options {
        print!("{}", options::markdown());
        return Ok(());
    }
    // Refused before any prompt, so a re-run fails at once.
    if !args.force && path.exists() {
        bail!(
            "{} already exists — pass --force to overwrite it",
            path.display()
        );
    }
    let onboarding = gather(args, std::io::stdin().is_terminal())?;
    // Every deployment signs: the box's keys are kept or minted into the secret store the code
    // root declares before the file naming their root is written.
    let secrets = suzatary_io::secrets::Secretspec::new(
        Instance::repo_beside(path, onboarding.repo()).join("secretspec.toml"),
    );
    let keys = keys(&secrets)?;
    let written = written(path, &onboarding, &keys.root().public(), env)?;
    println!("wrote {}", path.display());
    println!(
        "the box's and the agent's keys are in the secret store; the root is {}",
        written.root().as_str()
    );
    println!();
    println!("next, at a terminal:");
    for step in onboarding.steps(&written.reach()) {
        println!("{step}");
    }
    Ok(())
}

/// `onboarding`'s instance file written under `root`, its chains and topic stores begun, and read
/// back.
///
/// # Errors
/// The file not written, a chain or a topic store not begun, or the file not reading back.
fn written(
    path: &Path,
    onboarding: &Onboarding,
    root: &prodrome_core::sign::PublicKey,
    env: &crate::paths::Env,
) -> anyhow::Result<Instance> {
    onboarding
        .file(root)?
        .write(path, onboarding::HEADER, true)?;
    let data = Instance::data_beside(path, onboarding.data());
    begin_chains(&data)?;
    // The box-local tier, begun before anything is written to it, ignoring itself.
    let tier = data.join(crate::instance::Local::DIR);
    suzatary_io::tier::ignored(&tier).with_context(|| format!("beginning {}", tier.display()))?;
    let read = Instance::read(path, env)?;
    read.secretary
        .topics
        .open()
        .begin()
        .context("beginning the topic stores")?;
    Ok(read)
}

/// The onboarding these flags give off a terminal: how a deployment shown in the guide was
/// begun.
///
/// # Errors
/// A flag clap refuses, or an answer [`Onboarding::new`] does.
pub fn answered<'a>(flags: impl IntoIterator<Item = &'a str>) -> anyhow::Result<Onboarding> {
    let Flags { args } = Flags::try_parse_from(std::iter::once("init").chain(flags))?;
    gather(args, false)
}

/// [`Args`] as a command of their own, for [`answered`].
#[derive(clap::Parser)]
struct Flags {
    #[command(flatten)]
    args: Args,
}

/// Every answer, from its flag, the terminal, or a refusal naming the flag.
fn gather(args: Args, terminal: bool) -> anyhow::Result<Onboarding> {
    if terminal {
        let mut lines = std::io::stdin()
            .lines()
            .map(|line| line.map(|text| text.trim().to_owned()));
        let mut out = std::io::stdout();
        gather_from(args, &mut Asker::At(&mut lines, &mut out))
    } else {
        gather_from(args, &mut Asker::Off)
    }
}

/// Every answer, each asked of `ask` until it is one: a refused answer is asked again under
/// its refusal, and every answer already given is kept.
fn gather_from(args: Args, ask: &mut Asker<'_>) -> anyhow::Result<Onboarding> {
    let name = ask.until(args.name, "--name", "your name", |name| {
        onboarding::named(name).map(str::to_owned)
    })?;
    let zone = ask.until(
        args.zone,
        "--zone",
        "your time zone, by name (such as America/New_York)",
        zone_of,
    )?;
    let owner = ask.until(
        args.actor.or_else(|| actor_of(&name)),
        "--actor",
        "the actor your writes carry (a-z, 0-9, - and _)",
        onboarding::owner,
    )?;
    let logins = if args.logins.is_empty() {
        ask.lines("--login", "your Tailscale logins")?
    } else {
        args.logins
    };
    let box_name = ask.until(
        args.box_name,
        "--box",
        "this box's MagicDNS name (Self.DNSName in `tailscale status --json`)",
        BoxName::parse,
    )?;
    let door = Door::new(
        box_name,
        logins.into_iter().map(Login::from_whois).collect(),
    )?;
    let categories = if args.categories.is_empty() {
        onboarding::CATEGORIES.map(str::to_owned).to_vec()
    } else {
        args.categories
    };
    let email = match (args.email, args.mail_provider) {
        (None, Some(_)) => bail!("--mail-provider names who serves --email, and there is none"),
        (None, None) => ask.optional("--email", "your first mail address (blank for none)")?,
        (email, _) => email,
    };
    let mail = match email {
        None => None,
        Some(email) => Some(FirstAccount {
            provider: match args.mail_provider {
                Some(provider) => provider,
                None => ask.until(None, "--mail-provider", &providers(), provider_of)?,
            },
            email,
        }),
    };
    Onboarding::new(
        Person {
            name,
            city: args.city,
            zone,
        },
        owner,
        door,
        categories,
        mail,
        (args.repo, args.data),
    )
}

/// Where answers come from: a terminal's lines and the screen they are asked on, or nowhere.
enum Asker<'a> {
    /// A terminal.
    At(
        &'a mut dyn Iterator<Item = std::io::Result<String>>,
        &'a mut dyn std::io::Write,
    ),
    /// No terminal: a missing answer is refused by its flag.
    Off,
}

impl Asker<'_> {
    /// The answer `given` by its flag, or asked for until `parse` takes one; a flag's answer it
    /// refuses is refused, since nobody is there to ask again.
    fn until<T>(
        &mut self,
        given: Option<String>,
        flag: &str,
        question: &str,
        parse: impl Fn(&str) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        if let Some(given) = given {
            return parse(given.trim()).map_err(|refused| anyhow::anyhow!("{flag}: {refused:#}"));
        }
        loop {
            let answer = self.line(flag, question)?;
            match parse(&answer) {
                Ok(value) => return Ok(value),
                Err(refused) => self.say(&format!("{refused:#}"))?,
            }
        }
    }

    /// One answer, blank for none; off a terminal, none.
    fn optional(&mut self, flag: &str, question: &str) -> anyhow::Result<Option<String>> {
        match self {
            Asker::Off => Ok(None),
            Asker::At(..) => Ok(Some(self.line(flag, question)?).filter(|line| !line.is_empty())),
        }
    }

    /// One line, or off a terminal a refusal naming the flag.
    fn line(&mut self, flag: &str, question: &str) -> anyhow::Result<String> {
        let Asker::At(lines, out) = self else {
            bail!("no terminal to ask, and {flag} is missing — pass it directly");
        };
        write!(out, "{question}: ")?;
        out.flush()?;
        match lines.next() {
            Some(line) => Ok(line.context("reading from the terminal")?),
            None => bail!("the terminal closed before {flag} was answered"),
        }
    }

    /// Lines until a blank one, at least one.
    fn lines(&mut self, flag: &str, question: &str) -> anyhow::Result<Vec<String>> {
        if let Asker::Off = self {
            bail!("no terminal to ask, and {flag} is missing — pass it at least once ({question})");
        }
        self.say(&format!(
            "{question}; one per line, a blank line to finish:"
        ))?;
        let mut out = Vec::new();
        loop {
            let line = self.line(flag, " ")?;
            if line.is_empty() {
                if out.is_empty() {
                    self.say("at least one Tailscale login opens the app and writes as you")?;
                    continue;
                }
                break;
            }
            out.push(line);
        }
        Ok(out)
    }

    /// A line on the terminal's screen; nothing off one.
    fn say(&mut self, text: &str) -> anyhow::Result<()> {
        if let Asker::At(_, out) = self {
            writeln!(out, "{text}")?;
        }
        Ok(())
    }
}

/// The zone named, or why not with the zone the person most likely meant.
fn zone_of(text: &str) -> anyhow::Result<Zone> {
    Zone::named(text).map_err(|refused| match Zone::closest(text) {
        Some(closest) => anyhow::anyhow!("{refused}. Did you mean {}?", closest.name()),
        None => anyhow::anyhow!("{refused}"),
    })
}

/// The actor a name suggests: its first word in lower case, when that is an actor.
fn actor_of(name: &str) -> Option<String> {
    let first = name.split_whitespace().next()?.to_lowercase();
    Actor::new(first.clone()).ok().map(|_| first)
}

/// The providers as `--mail-provider` spells them, numbered.
fn providers() -> String {
    let names: Vec<String> = Provider::value_variants()
        .iter()
        .filter_map(clap::ValueEnum::to_possible_value)
        .enumerate()
        .map(|(at, value)| format!("{} {}", at + 1, value.get_name()))
        .collect();
    format!("who serves it: {} (a number or a name)", names.join(", "))
}

/// A provider by its number, its name in any case, or why not with the one a common name
/// means (`outlook` → `microsoft-personal`).
fn provider_of(answer: &str) -> anyhow::Result<Provider> {
    let variants = Provider::value_variants();
    if let Some(chosen) = answer
        .parse::<usize>()
        .ok()
        .and_then(|number| number.checked_sub(1))
        .and_then(|at| variants.get(at))
    {
        return Ok(*chosen);
    }
    Provider::from_str(answer, true).map_err(|_| {
        let meant = match answer.to_lowercase().as_str() {
            "outlook" | "hotmail" | "live" | "msn" | "outlook.com" => Some("microsoft-personal"),
            "office" | "office365" | "microsoft365" | "m365" | "exchange" | "microsoft" => {
                Some("microsoft-work")
            }
            "google" | "googlemail" | "google mail" => Some("gmail"),
            "protonmail" | "proton mail" => Some("proton"),
            _ => None,
        };
        match meant {
            Some(name) => anyhow::anyhow!("{answer:?} is not a provider. Did you mean {name}?"),
            None => anyhow::anyhow!("{answer:?} is not a provider"),
        }
    })
}

/// Each chain begun with its `Genesis`, the prodrome every writer writes into.
fn begin_chains(data: &Path) -> anyhow::Result<()> {
    for (_, events) in onboarding::CHAINS {
        let chain = suzatary_core::record::EventStore::new(
            data.join(events),
            prodrome_core::policy::Untrusted::none(),
        );
        // A chain `--force` finds begun stays the chain it is.
        if chain
            .tips()
            .with_context(|| format!("reading {events}"))?
            .is_empty()
        {
            chain
                .init(suzatary_core::record::GENESIS)
                .with_context(|| format!("beginning {events}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory as _, Parser};
    use proptest::prelude::*;

    #[derive(Debug, Parser)]
    struct Wrapper {
        #[command(flatten)]
        args: Args,
    }

    fn parse(argv: &[&str]) -> Args {
        Wrapper::parse_from(std::iter::once("init").chain(argv.iter().copied())).args
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-init-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp dir");
        dir
    }

    /// An environment beneath a test's own directory.
    fn scratch_env(dir: &std::path::Path) -> crate::paths::Env {
        crate::paths::Env::under(dir.join("home"), dir.join("run"))
    }

    const ADA: &[&str] = &[
        "--name",
        "Ada Lovelace",
        "--box",
        "ada-box.tailnet.ts.net",
        "--zone",
        "Europe/London",
        "--login",
        "her@example.test",
    ];

    fn person() -> impl Strategy<Value = Person> {
        (
            "\\PC{0,10}[^\\s\\pC]\\PC{0,10}",
            "\\PC{0,16}",
            prop::sample::select(vec![
                "UTC",
                "America/New_York",
                "Europe/London",
                "Europe/Berlin",
            ]),
        )
            .prop_map(|(name, city, zone)| Person {
                name,
                city,
                zone: Zone::named(zone).expect("a zone"),
            })
    }

    fn onboarding() -> impl Strategy<Value = Onboarding> {
        (
            person(),
            "[a-z][a-z0-9_-]{0,9}".prop_filter("never an agent", |name| {
                name != "assistant" && name != "triage"
            }),
            (
                "[a-z][a-z0-9-]{0,8}[a-z0-9]\\.tailnet\\.ts\\.net",
                prop::collection::btree_set("[a-z]{1,8}@example\\.test", 1..4),
            ),
            // Distinct names in any order, since a category's rank is its position.
            prop::collection::btree_set("[a-z]{1,8}", 1..6)
                .prop_map(|names| names.into_iter().collect::<Vec<_>>())
                .prop_shuffle(),
            prop::option::of((
                prop::sample::select(Provider::value_variants()),
                "[a-z]{1,8}@[a-z]{1,6}\\.test",
            )),
            (
                prop::sample::select(vec![".", "app", "/srv/app"]),
                prop::sample::select(vec![".", "data", "../user"]),
            ),
        )
            .prop_map(
                |(person, owner, (name, logins), categories, mail, (repo, data))| {
                    Onboarding::new(
                        person,
                        Actor::new(owner).expect("generated an actor"),
                        Door::new(
                            BoxName::parse(&name).expect("generated a name"),
                            logins.into_iter().map(Login::from_whois).collect(),
                        )
                        .expect("generated logins"),
                        categories,
                        mail.map(|(provider, email)| FirstAccount { provider, email }),
                        (PathBuf::from(repo), PathBuf::from(data)),
                    )
                    .expect("a generated onboarding is one")
                },
            )
    }

    proptest! {
        /// The file `init` writes reads back to the onboarding it was written from, its print
        /// is a fixed point of reading and printing, and the binary's startup checks pass it.
        #[test]
        fn the_written_file_reads_back_to_the_same_value(onboarding in onboarding()) {
            let printed = onboarding
                .printed(&suzatary_core::cast::keys::root())
                .expect("prints");
            let read: crate::instance::File = toml::from_str(&printed).expect("parses");
            prop_assert_eq!(onboarding::of(&read), Some(onboarding));
            prop_assert_eq!(read.to_toml_string(onboarding::HEADER).expect("prints"), printed.clone());
            let dir = temp_dir("law");
            let path = dir.join("instance.toml");
            std::fs::write(&path, &printed).expect("written");
            let started = Instance::read(&path, &scratch_env(&dir));
            prop_assert!(started.is_ok(), "{:?}", started.err());
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// A fresh init begins both chains, each holding its `Genesis` alone.
    #[test]
    fn init_begins_every_chain() {
        let dir = temp_dir("chains");
        let path = dir.join("instance.toml");
        let onboarding = gather(parse(ADA), false).expect("gathered");
        let read = written(
            &path,
            &onboarding,
            &suzatary_core::cast::keys::root(),
            &scratch_env(&dir),
        )
        .expect("what init wrote, init can read");
        assert_eq!(read.root(), &suzatary_core::cast::keys::root());
        assert_eq!(read.deployment.chains.names(), vec!["life", "suzatary"]);
        for chain in &read.chains {
            let begun = suzatary_core::record::EventStore::new(
                &chain.events,
                prodrome_core::policy::Untrusted::none(),
            );
            let objects = begun.dag().expect("reads").objects().clone();
            assert_eq!(objects.len(), 1, "{:?}", chain.events);
            assert!(matches!(
                objects.values().next(),
                Some(prodrome_core::event::Envelope::Genesis(_))
            ));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The owner's actor defaults to their first name, and an agent's name is refused.
    #[test]
    fn the_owner_is_named_by_their_first_name_and_never_as_an_agent() {
        let ada = gather(parse(ADA), false).expect("gathered");
        let file = ada
            .file(&suzatary_core::cast::keys::root())
            .expect("a file");
        let owner: Vec<&str> = file.web.act_as.values().map(Actor::as_str).collect();
        assert_eq!(owner, vec!["ada"]);

        let mut argv = ADA.to_vec();
        argv.extend(["--actor", "triage"]);
        let refused = gather(parse(&argv), false).expect_err("an agent's actor");
        assert!(refused.to_string().contains("agent"), "{refused}");
    }

    /// A second init refuses without `--force`: an existing file is somebody's deployment.
    #[test]
    fn a_second_init_refuses_without_force() {
        let dir = temp_dir("noforce");
        let path = dir.join("instance.toml");
        let onboarding = gather(parse(ADA), false).expect("gathered");
        written(
            &path,
            &onboarding,
            &suzatary_core::cast::keys::root(),
            &scratch_env(&dir),
        )
        .expect("the first init");
        let keyed = |args| {
            run_keyed(
                &path,
                args,
                &scratch_env(&dir),
                super::super::device::kept_or_minted_in,
            )
        };
        let refused = keyed(parse(ADA)).expect_err("the second refuses");
        assert!(refused.to_string().contains("--force"), "{refused}");
        let mut argv = ADA.to_vec();
        argv.push("--force");
        // Past the refusal: with no secret store here, what stops it is the box's keys.
        let past = keyed(parse(&argv)).map_or_else(|e| format!("{e:#}"), |()| String::new());
        assert!(!past.contains("--force"), "{past}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No flag takes a secret, so no secret is ever an argument: every one is set at
    /// `secretspec`'s prompt, which the steps say, after the token is minted.
    #[test]
    fn no_secret_is_an_argument_and_the_steps_say_where_each_is_typed() {
        for argument in Wrapper::command().get_arguments() {
            let id = argument.get_id().as_str().to_lowercase();
            for word in ["token", "key", "password", "secret"] {
                assert!(!id.contains(word), "--{id} looks like a secret");
            }
        }
        let commands = |argv: &[&str], reach: &Reach| -> Vec<String> {
            gather(parse(argv), false)
                .expect("gathered")
                .steps(reach)
                .into_iter()
                .map(|step| step.command)
                .collect()
        };
        let installed = Reach::Installed;
        assert_eq!(
            commands(ADA, &installed),
            [
                "claude setup-token",
                "secretspec set CLAUDE_CODE_OAUTH_TOKEN --file \"$(suzatary secretspec)\"",
                "suzatary doctor"
            ]
        );
        let mut argv = ADA.to_vec();
        argv.extend([
            "--email",
            "her@study.test",
            "--mail-provider",
            "microsoft-work",
        ]);
        assert_eq!(
            commands(&argv, &installed),
            [
                "claude setup-token",
                "secretspec set CLAUDE_CODE_OAUTH_TOKEN --file \"$(suzatary secretspec)\"",
                "suzatary auth her@study.test",
                "suzatary auth her@study.test --scope smtp",
                "suzatary doctor"
            ]
        );
        // From a user repository every step goes through its script, which holds the tools.
        assert_eq!(
            commands(ADA, &Reach::Script("./suzatary".to_owned())),
            [
                "./suzatary tool claude setup-token",
                "./suzatary secret CLAUDE_CODE_OAUTH_TOKEN",
                "./suzatary doctor"
            ]
        );
        // A Proton account's steps are its template's, spelled the same way.
        let mut proton = ADA.to_vec();
        proton.extend(["--email", "her@proton.test", "--mail-provider", "proton"]);
        assert_eq!(
            commands(&proton, &Reach::Script("./suzatary".to_owned())),
            [
                "./suzatary tool claude setup-token",
                "./suzatary secret CLAUDE_CODE_OAUTH_TOKEN",
                "./suzatary tool protonmail-bridge --cli",
                "./suzatary secret PROTON_BRIDGE_PASSWORD",
                "$EDITOR mbsyncrc",
                "./suzatary doctor"
            ]
        );
    }
}
