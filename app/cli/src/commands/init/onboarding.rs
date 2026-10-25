//! What a person tells the box when they start, and the deployment it means.
//!
//! [`Onboarding`] is the whole of `suzatary init`'s input; [`Onboarding::file`] is the
//! `instance.toml` it denotes and [`Onboarding::steps`] the acts only the person can do (a
//! secret typed at a prompt, an account's sign-in), printed and never run. Both are functions
//! of the value, so the file and the advice cannot disagree.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::bail;
use prodrome_core::event::Actor;
use suzatary_core::authoring::Writer;
use suzatary_core::capability::Login;
use suzatary_core::chains::ChainName;
use suzatary_core::connect::{AccountKind, Connection, Params, Reach, Refused};
use suzatary_core::mail::account::{Authority, MailAccount, Roster};
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::persona::Persona;

use crate::instance::{Chain, File, Mail, Secretary, Serving, Web};

/// Everything `init` asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Onboarding {
    /// Who the deployment serves.
    person: Person,
    /// The actor their own writes carry; never an agent's.
    owner: Actor,
    /// Where the app answers, and who may come in.
    door: Door,
    /// The todos' categories, most important first. Never empty, no name twice.
    categories: Vec<String>,
    /// The first mail account, if any; more are added to the file by hand.
    mail: Option<FirstAccount>,
    /// The code root, relative to the instance file.
    repo: PathBuf,
    /// The data root, relative to the instance file.
    data: PathBuf,
}

/// The app's door: the box's name on the tailnet, and the person's logins, each of which may
/// open the app and writes as its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Door {
    name: BoxName,
    logins: BTreeSet<Login>,
}

/// A box's `MagicDNS` name (`<box>.<tailnet>.ts.net`), the one an `https://` URL to the app names
/// and `tailscale cert` mints a certificate for: lower-case DNS labels, at least two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxName(String);

impl BoxName {
    /// The name, or why this text is not one.
    ///
    /// # Errors
    /// Not two or more labels of `a-z`, `0-9` and inner `-`; the one final dot the daemon
    /// prints a name with is dropped.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let text = text.strip_suffix('.').unwrap_or(text);
        let label = |part: &str| {
            !part.is_empty()
                && part.len() <= 63
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !part.starts_with('-')
                && !part.ends_with('-')
        };
        let parts: Vec<&str> = text.split('.').collect();
        if parts.len() < 2 || !parts.iter().all(|part| label(part)) {
            bail!(
                "{text:?} is not a MagicDNS name; `tailscale status --json` prints this box's \
                 as `Self.DNSName` (like `box.tailnet.ts.net.`)"
            );
        }
        Ok(Self(text.to_owned()))
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Door {
    /// The door, or why it opens for nobody.
    ///
    /// # Errors
    /// No login.
    pub fn new(name: BoxName, logins: BTreeSet<Login>) -> anyhow::Result<Self> {
        if logins.is_empty() {
            bail!("at least one Tailscale login opens the app and writes as you");
        }
        Ok(Self { name, logins })
    }

    /// The app's address: https on the box's name, where the module serves it.
    #[must_use]
    pub fn url(&self) -> String {
        format!("https://{}", self.name.as_str())
    }
}

/// `[persona]`'s three asked fields; the rest of it is personal prose, written by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    /// How the manuals address them.
    pub name: String,
    /// The city `suzatary weather` forecasts; empty for none.
    pub city: String,
    /// Their time zone, by name.
    pub zone: suzatary_core::zone::Zone,
}

/// A first account: an address and who serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstAccount {
    /// Who serves it.
    pub provider: Provider,
    /// The address.
    pub email: String,
}

/// The mail providers a first account may be with, each one sync strategy and its defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Provider {
    /// Gmail, through lieer.
    Gmail,
    /// A work or school Microsoft 365 account, over IMAP.
    MicrosoftWork,
    /// A personal Outlook.com account, over IMAP.
    MicrosoftPersonal,
    /// Proton Mail, through its Bridge.
    Proton,
}

/// One act left to the person, as the command they type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// What to run.
    pub command: String,
    /// Why, in a few words.
    pub why: &'static str,
}

/// The header every written file carries: what a first-time reader needs, not a comment
/// per field.
pub(crate) const HEADER: &str = "\
# WHAT MAKES THIS DEPLOYMENT YOURS, written by `suzatary init` (docs/guide/).
#
# It holds nothing secret: every secret is set at a prompt with
# `secretspec set`, and the actors below are in every object's `actor` field
# anyway, so this file is committed with the data it names.
#
# `[web]` is the door: only these Tailscale logins open the app, from your
# own tailnet. Nothing here opens a public endpoint, and nothing should.
#
# `[[chains]]` IS AN ARRAY WHOSE ORDER IS MEANING: the first is what every
# verb with no `--chain` and the bare `/api/...` paths mean, so your life
# stays first. Anything about the system itself (a failing sync, a verb to
# fix) goes to `suzatary`.
";

/// The two chains every deployment begins with, each beside its agent's replica: the person's
/// life first, since the first is what every unscoped verb means, then the system's own work.
pub const CHAINS: [(&str, &str, &str); 2] = [
    ("life", "events", "events-triage"),
    ("suzatary", "events-suzatary", "events-triage-suzatary"),
];

/// The categories a deployment starts with when none are named.
pub const CATEGORIES: [&str; 5] = ["work", "study", "project", "home", "other"];

/// Where `suzatary serve` run by hand binds: loopback, where tailscaled names no one, so the gate
/// admits nothing there. The module's unit serves the box's tailnet address, on the door's URL.
const HOST: &str = "127.0.0.1";
const PORT: u16 = 8787;

impl Onboarding {
    /// The value, or what is wrong with the answers.
    ///
    /// # Errors
    /// No name, an owner spelled as an agent, no category or one named twice.
    pub fn new(
        person: Person,
        owner: Actor,
        door: Door,
        categories: Vec<String>,
        mail: Option<FirstAccount>,
        roots: (PathBuf, PathBuf),
    ) -> anyhow::Result<Self> {
        named(&person.name)?;
        owner_not_an_agent(&owner)?;
        if categories.is_empty() {
            bail!("the todos need at least one category");
        }
        let mut seen = BTreeSet::new();
        if let Some(twice) = categories.iter().find(|name| !seen.insert(*name)) {
            bail!("the category {twice:?} is named twice");
        }
        Ok(Self {
            person,
            owner,
            door,
            categories,
            mail,
            repo: roots.0,
            data: roots.1,
        })
    }

    /// The code root, relative to the instance file.
    #[must_use]
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// The data root, relative to the instance file.
    #[must_use]
    pub fn data(&self) -> &Path {
        &self.data
    }

    /// The instance file this onboarding means.
    ///
    /// # Errors
    /// An address the roster refuses.
    pub(crate) fn file(&self, root: &prodrome_core::sign::PublicKey) -> anyhow::Result<File> {
        let roster: Vec<Actor> = std::iter::once(self.owner.clone())
            .chain(agents())
            .collect();
        let chains = CHAINS
            .iter()
            .map(|(name, events, proposals)| {
                Ok(Chain {
                    name: ChainName::new(*name)?,
                    events: PathBuf::from(events),
                    proposals: PathBuf::from(proposals),
                    // An agent's writes land in its replica, never on a chain.
                    untrusted: Vec::new(),
                    actors: roster.clone(),
                    owner: self.owner.clone(),
                })
            })
            .collect::<anyhow::Result<_>>()?;
        let web = Web {
            viewers: self.door.logins.iter().cloned().collect(),
            app_url: self.door.url(),
            act_as: self
                .door
                .logins
                .iter()
                .map(|login| (login.clone(), self.owner.clone()))
                .collect(),
            serving: Serving {
                host: HOST.to_owned(),
                port: PORT,
                tls_dir: PathBuf::from("/var/lib/suzatary/tls"),
                dist: PathBuf::from("web/dist"),
                briefings: PathBuf::from("briefings"),
            },
        };
        let mail = Mail {
            root: PathBuf::from("Mail"),
            state: PathBuf::from("state.json"),
            notmuch: PathBuf::from("notmuch"),
            // None: a search is refused rather than reading the box's own notmuch config.
            notmuch_config: None,
            mbsyncrc: PathBuf::from("mbsyncrc"),
            // Under the data root: an absent heartbeat reads as "triage has not run", so a
            // tmpfs path would make every reboot look like an outage.
            heartbeat: PathBuf::from(".local/state/triage-heartbeat"),
            accounts: Roster::new(
                self.mail
                    .iter()
                    .map(FirstAccount::account)
                    .collect::<Result<_, _>>()?,
            )?,
            junk: suzatary_core::mail::junk::ChurnList::default(),
        };
        Ok(File {
            repo: self.repo.clone(),
            data: self.data.clone(),
            chains,
            web,
            categories: self.categories.iter().cloned().zip(0..).collect(),
            mail,
            secretary: Secretary::default(),
            agent: crate::instance::Triage::default(),
            persona: Persona {
                name: self.person.name.clone(),
                city: self.person.city.clone(),
                zone: self.person.zone,
                prose: suzatary_core::persona::Prose::default(),
            },
            notify: None,
            coding: None,
            cache: None,
            plugins: Vec::new(),
            signatures: crate::instance::Signatures { root: root.clone() },
        })
    }

    /// The file as `init` writes it under `root`, header and all.
    ///
    /// # Errors
    /// An address the roster refuses, as for the file it prints.
    pub fn printed(&self, root: &prodrome_core::sign::PublicKey) -> anyhow::Result<String> {
        self.file(root)?.to_toml_string(HEADER)
    }

    /// What the person does next, in order: the model's secret, the account's sign-in, and
    /// `doctor` to see it all read back, each spelled as `reach` reaches the app.
    #[must_use]
    pub fn steps(&self, reach: &Reach) -> Vec<Step> {
        model_steps(reach)
            .into_iter()
            .chain(self.mail.iter().flat_map(|first| first.steps(reach)))
            .chain([Step {
                command: reach.app("doctor"),
                why: "what it can do, and what it read",
            }])
            .collect()
    }
}

/// The name to address the person by, trimmed, or why not.
///
/// # Errors
/// A blank name.
pub fn named(text: &str) -> anyhow::Result<&str> {
    let name = text.trim();
    if name.is_empty() {
        bail!("a name to address you by is the one thing the manuals need");
    }
    Ok(name)
}

/// The owner's actor spelled `text`, or why not.
///
/// # Errors
/// Not an actor, or an agent's.
pub fn owner(text: &str) -> anyhow::Result<Actor> {
    let owner =
        Actor::new(text).map_err(|failed| anyhow::anyhow!("{text:?} is not an actor: {failed}"))?;
    owner_not_an_agent(&owner)?;
    Ok(owner)
}

fn owner_not_an_agent(owner: &Actor) -> anyhow::Result<()> {
    if agents().any(|agent| agent == *owner) {
        bail!(
            "{:?} is an agent's actor; your writes would read as its",
            owner.as_str()
        );
    }
    Ok(())
}

/// The agents' actors, on every chain's roster beside the owner's.
fn agents() -> impl Iterator<Item = Actor> {
    [Writer::Assistant, Writer::Triage]
        .into_iter()
        .map(|writer| Actor::new(writer.actor()).expect("an agent's name is an actor"))
}

impl FirstAccount {
    /// The roster entry: the mail template of its provider applied to its address, so `init`
    /// writes the account a Connect card would.
    ///
    /// # Errors
    /// An address that is not one.
    pub fn account(&self) -> Result<MailAccount, Refused> {
        let (kind, params) = self.applied()?;
        kind.account(&params)
    }

    /// The connection a Connect card would make of it.
    ///
    /// # Errors
    /// An address that is not one.
    pub fn connection(&self) -> Result<Connection, Refused> {
        let (kind, params) = self.applied()?;
        Connection::new(kind.template(), params)
    }

    /// Its provider's mail template, and the parameters its address gives it.
    fn applied(&self) -> Result<(AccountKind, Params), Refused> {
        let email = EmailAddress::new(&self.email).map_err(|refused| Refused::Param {
            param: "email",
            said: refused.message().to_owned(),
        })?;
        let microsoft = |tenant| Params::Microsoft {
            email: email.clone(),
            tenant,
        };
        let (kind, params) = match self.provider {
            Provider::Gmail => (AccountKind::Gmail, Params::Address(email.clone())),
            Provider::MicrosoftWork => {
                (AccountKind::Microsoft, microsoft(Authority::Organizations))
            }
            Provider::MicrosoftPersonal => {
                (AccountKind::Microsoft, microsoft(Authority::Consumers))
            }
            Provider::Proton => (AccountKind::Proton, Params::Address(email.clone())),
        };
        Ok((kind, params))
    }

    /// The sign-in this account needs, given at a terminal by the person, never by the box: its
    /// template's terminal steps, and a Microsoft account's two consents, which `auth` gives.
    #[must_use]
    pub fn steps(&self, reach: &Reach) -> Vec<Step> {
        match self.provider {
            Provider::MicrosoftWork | Provider::MicrosoftPersonal => vec![
                Step {
                    command: reach.app(&format!("auth {}", self.email)),
                    why: "the device-code sign-in, for reading",
                },
                Step {
                    command: reach.app(&format!("auth {} --scope smtp", self.email)),
                    why: "a separate consent, for sending",
                },
            ],
            Provider::Gmail | Provider::Proton => self
                .connection()
                .map(|connection| connection.steps())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|step| match step {
                    suzatary_core::connect::Step::Terminal { run, why } => Some(Step {
                        command: run.spelled(reach),
                        // Lieer waits for Google's redirect to the box's own localhost.
                        why: if self.provider == Provider::Gmail {
                            "lieer's sign-in, in a browser; on a box with no browser, open the \
                             SSH tunnel in AGENT.md §4.5 first"
                        } else {
                            why
                        },
                    }),
                    _ => None,
                })
                .collect(),
        }
    }
}

/// What the Claude subscription needs: a token, set at a prompt so it is never an argument.
#[must_use]
pub fn model_steps(reach: &Reach) -> Vec<Step> {
    vec![
        Step {
            command: reach.tool("claude setup-token"),
            why: "a year's token for the unattended jobs; or skip this step and the next: the \
                  app asks you to connect the model, from your phone",
        },
        Step {
            command: reach.secret(suzatary_io::model::TOKEN),
            why: "typed at the prompt, never on the command line",
        },
    ]
}

impl std::fmt::Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "  # {}\n  {}", self.why, self.command)
    }
}

/// The onboarding a file is, when it is one: the inverse of [`Onboarding::file`] on its image.
#[cfg(test)]
pub(crate) fn of(file: &File) -> Option<Onboarding> {
    use std::collections::BTreeMap;
    let owners: BTreeSet<&Actor> = file.web.act_as.values().collect();
    let owner = (*owners.iter().next()?).clone();
    let logins: BTreeSet<Login> = file.web.act_as.keys().cloned().collect();
    let viewers: BTreeSet<Login> = file.web.viewers.iter().cloned().collect();
    let ranked: BTreeMap<u32, &String> = file
        .categories
        .iter()
        .map(|(name, rank)| (*rank, name))
        .collect();
    let mail = match file.mail.accounts.accounts() {
        [] => None,
        [account] => Some(
            [
                Provider::Gmail,
                Provider::MicrosoftWork,
                Provider::MicrosoftPersonal,
                Provider::Proton,
            ]
            .into_iter()
            .map(|provider| FirstAccount {
                provider,
                email: account.email.to_string(),
            })
            .find(|first| first.account().as_ref() == Ok(account))?,
        ),
        _ => return None,
    };
    let name = BoxName::parse(file.web.app_url.strip_prefix("https://")?).ok()?;
    (owners.len() == 1
        && file.chains.iter().all(|chain| chain.owner == owner)
        && logins == viewers
        && ranked.len() == file.categories.len()
        && ranked.keys().copied().zip(0..).all(|(rank, at)| rank == at))
    .then(|| Onboarding {
        person: Person {
            name: file.persona.name.clone(),
            city: file.persona.city.clone(),
            zone: file.persona.zone,
        },
        owner,
        door: Door { name, logins },
        categories: ranked.into_values().cloned().collect(),
        mail,
        repo: file.repo.clone(),
        data: file.data.clone(),
    })
}
