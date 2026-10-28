//! What is connected, by group: the Settings page's one reading, a pure function of what the
//! instance declares and what the box's stores observed of it.
//!
//! The overview is the free join-semilattice on (group, line) pairs, read grouped: each table of
//! the instance contributes its lines, [`Overview::join`] is their union, and no line depends on
//! the order its table was written in. A group nothing is connected to holds no line. A line
//! carries the owner's words only: before it leaves, every secret's name in it is replaced by
//! what the owner calls that secret ([`Help::label`]), and its open cards travel as references,
//! so a key name reaches no reader.

use std::collections::{BTreeMap, BTreeSet};

use crate::capability::{MailCredential, MailGrants, SecretState};
use crate::connect::{self, Checked, Connection, Group, Key, Standing};
use crate::deployment::Deployment;
use crate::health::{Health, Healths};
use crate::mail::account::MailAccount;
use crate::plugin::{Manifest, PluginName};
use crate::secretary::acts::Act;
use crate::secretary::store::Proposal;
use crate::settings::Help;
use crate::settings::remote::Pushed;
use crate::source_key::SourceKey;
use crate::wire;

/// What an open card asks of the box, and so which lines it is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Need {
    /// A mail account's sign-in, by its address. Mail drafted from it asks for no connection, so
    /// a waiting draft never stands in for the sign-in.
    MailSignIn(String),
    /// A Microsoft account's consent to send, by its address: a step after its sign-in to read,
    /// so a card of its own.
    MailSending(String),
    /// The box's Google client, which every Google account signs in through: drawn on the first
    /// Google account's line.
    GoogleClient,
    /// A secret, by its stored name, which is matched here and never leaves.
    Secret(String),
    /// Connecting the subscription's model: starting its sign-in.
    SignIn,
    /// The code an open sign-in waits for: a step after the sign-in, so a card of its own.
    SignInCode,
    /// Taking a connection out, by its key.
    Removal(Key),
    /// Letting a plugin reach an origin a link of it was served from, drawn under the card that
    /// asks for the link.
    Grant(PluginName),
}

/// One open card of the Inbox's: what it asks for, and the reference it is drawn by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Asking {
    /// What it asks for.
    pub need: Need,
    /// When it was first asked, ISO 8601, UTC: the earliest card asking for a need is the one
    /// drawn.
    pub at: String,
    /// Its `proposal:<conversation>/<n>`.
    pub card: String,
}

impl Asking {
    /// Every card still waiting in `inbox` whose act is about a connection.
    #[must_use]
    pub fn open(inbox: &crate::secretary::inbox::Fold) -> Vec<Self> {
        inbox
            .objects()
            .filter(|object| object.reading.open())
            .filter_map(|object| {
                let need = match &object.asked.proposal {
                    Proposal::Act(Act::AuthSend { account }) => {
                        Need::MailSending(account.to_string())
                    }
                    Proposal::Act(Act::Auth { account } | Act::GoogleSignIn { account, .. }) => {
                        Need::MailSignIn(account.to_string())
                    }
                    Proposal::Act(Act::GoogleClient { .. }) => Need::GoogleClient,
                    Proposal::Act(Act::Secret { name, .. }) => Need::Secret(name.clone()),
                    Proposal::Act(Act::SignIn { .. }) => Need::SignIn,
                    Proposal::Act(Act::SignInCode { .. }) => Need::SignInCode,
                    Proposal::Act(Act::Disconnect { connection }) => {
                        Need::Removal(connection.key())
                    }
                    Proposal::Act(Act::Grant { plugin, .. }) => Need::Grant(plugin.clone()),
                    // These cards stand on no line of the page.
                    Proposal::Act(
                        Act::Connect { .. }
                        | Act::UnlockStore { .. }
                        | Act::Web { .. }
                        | Act::Patch { .. }
                        | Act::DriveRead { .. }
                        | Act::CodingSession { .. }
                        | Act::Device(_)
                        | Act::Erase { .. }
                        | Act::CalendarEvent { .. },
                    )
                    | Proposal::Email(_)
                    | Proposal::Forward(_)
                    | Proposal::Changes(_)
                    | Proposal::Remember { .. }
                    | Proposal::Decide(_) => return None,
                };
                Some(Self {
                    need,
                    at: object.at.isoformat(),
                    card: object.asked.asked.to_string(),
                })
            })
            .collect()
    }
}

/// What a line is, once in its group: the identity two contributions of one thing share.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Repository,
    Account(String),
    Credential(PluginName, String),
    Plugin(PluginName),
    Drive,
    Job(String),
    Channel(connect::Channel),
    Routine,
    Build,
}

impl Slot {
    /// Its print, unique within its group and never a secret's name.
    fn key(&self) -> String {
        match self {
            Self::Repository => "repository".to_owned(),
            Self::Account(email) => format!("account:{email}"),
            Self::Credential(plugin, label) => format!("credential:{plugin}/{label}"),
            Self::Plugin(plugin) => format!("plugin:{plugin}"),
            Self::Drive => "drive".to_owned(),
            Self::Job(label) => format!("job:{label}"),
            Self::Channel(name) => format!("channel:{name}"),
            Self::Routine => "routine".to_owned(),
            Self::Build => "build".to_owned(),
        }
    }
}

/// What a plugin's cache-tier store is, as its declared schema reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Store {
    /// Nothing has begun it: the plugin has not synced.
    #[default]
    NotBegun,
    /// A store the calendar reads.
    Calendar,
    /// A store of another schema.
    Other,
    /// A store whose schema could not be read, and why.
    Unreadable(String),
}

/// Where the data is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Storage {
    /// The data repository on this box.
    pub root: String,
    /// Where it is pushed.
    pub pushed: Pushed,
}

/// What the instance declares, as far as the overview reads it.
#[derive(Clone, Copy)]
pub struct Declared<'a> {
    /// The chains and `[coding]`.
    pub deployment: &'a Deployment,
    /// Every secret name declared now, which a new connection widens with no restart.
    pub settables: &'a crate::settings::Settables,
    /// `[notify].channels`.
    pub channels: &'a [connect::Channel],
    /// Every mail account, a roster by construction.
    pub accounts: &'a [MailAccount],
    /// Each job the deployment runs a model for, in the order the page lists them.
    pub jobs: &'a [ModelJob],
    /// Every `[[plugins]]` manifest.
    pub plugins: &'a [Manifest],
    /// `[secretary] drive_allow`.
    pub drive: &'a [crate::drive::DrivePath],
    /// Where the data is kept.
    pub storage: &'a Storage,
    /// The program this box runs.
    pub build: &'a wire::AboutPayload,
    /// How the person reaches the app at a terminal, which a step's command is spelled in.
    pub reach: &'a connect::Reach,
}

/// One job the deployment runs a model for, as the deployment names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelJob {
    /// What the owner calls the job.
    pub label: String,
    /// What it runs, in the owner's words.
    pub detail: String,
    /// Whether the box records its calls.
    pub calls: Calls,
}

/// Whether the box records a job's calls, and so whether its line stands as they answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Calls {
    /// Each call is joined into the model health under `key`, and the model reads the secret
    /// `credential`, which its sign-in stores.
    Recorded {
        /// The model health's key: the model's name as [`crate::capability::Model::name`]
        /// spells it.
        key: String,
        /// The secret the model reads.
        credential: String,
    },
    /// No call is recorded, so the line says only what runs.
    Unrecorded,
}

/// What the box's stores say of it now; a name or account they hold nothing about reads as
/// never stored and never observed.
#[derive(Debug, Clone, Copy)]
pub struct Observed<'a> {
    /// Where each declared secret stands.
    pub secrets: &'a BTreeMap<String, SecretState>,
    /// Each Microsoft account's grants, by address.
    pub grants: &'a BTreeMap<String, MailGrants>,
    /// What the box's registers hold.
    pub registers: Registers<'a>,
}

impl Observed<'_> {
    /// What `sources` read: checked once any has succeeded, and standing as the join of their
    /// readings, unobserved while nothing has reported any.
    fn read(&self, sources: &[SourceKey]) -> (Checked, Standing) {
        let healths: Vec<&Health> = sources
            .iter()
            .filter_map(|key| self.registers.health.health(key))
            .collect();
        let checked = Checked::of(
            healths
                .iter()
                .copied()
                .find(|health| health.succeeded().is_some()),
        );
        let standing = healths
            .iter()
            .map(|health| Standing::of_health(Some(health.reading())))
            .reduce(Standing::join)
            .unwrap_or(Standing::Unobserved);
        (checked, standing)
    }

    fn secret(&self, name: &str) -> Standing {
        Standing::of_secret(
            name,
            self.secrets
                .get(name)
                .copied()
                .unwrap_or(SecretState::Missing),
        )
    }
}

/// What the box's registers hold, before the secret store is asked: each observation a line's
/// check reads.
#[derive(Debug, Clone, Copy)]
pub struct Registers<'a> {
    /// Every source's results.
    pub health: &'a Healths<SourceKey>,
    /// Each model's calls, by its name.
    pub answered: &'a Healths<String>,
    /// Each plugin's store.
    pub plugins: &'a BTreeMap<PluginName, Store>,
    /// The data repository's pushes, as its reflogs record them; `None` with no upstream.
    pub pushes: Option<&'a crate::health::Health>,
}

/// The connections the instance's tables hold, read through what the box observed: a line that
/// is a connection stands where the connection's own reading says, joined with the line's.
struct Connected<'a> {
    connections: &'a BTreeMap<Key, Connection>,
    observed: &'a Observed<'a>,
    reach: &'a connect::Reach,
}

impl Connected<'_> {
    fn connection(&self, group: Group, name: &str) -> Option<&Connection> {
        self.connections.get(&Key {
            group,
            name: name.to_owned(),
        })
    }

    /// Where `connection`'s steps stand.
    fn of(&self, connection: &Connection, checked: Checked) -> Standing {
        let secret = |name: &str| {
            self.observed
                .secrets
                .get(name)
                .copied()
                .unwrap_or(SecretState::Missing)
        };
        let signed_in = |email: &crate::mail::address::EmailAddress| {
            self.observed
                .grants
                .get(email.as_str())
                .is_some_and(|grants| grants.imap)
        };
        let may_send = |email: &crate::mail::address::EmailAddress| {
            self.observed
                .grants
                .get(email.as_str())
                .is_some_and(|grants| grants.smtp)
        };
        connection.standing(&connect::Seen {
            secret: &secret,
            signed_in: &signed_in,
            may_send: &may_send,
            checked,
            reach: self.reach,
        })
    }

    /// Where the connection keyed `group`/`name` stands; in place where the tables hold none.
    fn standing(&self, group: Group, name: &str, checked: Checked) -> Standing {
        self.connection(group, name)
            .map_or(Standing::Kept, |connection| self.of(connection, checked))
    }
}

/// One line before it is put in words.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Line {
    label: String,
    detail: Option<String>,
    standing: Standing,
    needs: BTreeSet<Need>,
}

/// What is connected: the free join-semilattice on (group, line).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overview(BTreeMap<(Group, Slot), Line>);

impl Overview {
    fn line(group: Group, slot: Slot, line: Line) -> Self {
        Self(BTreeMap::from([((group, slot), line)]))
    }

    /// The union, a line both hold being the greater of the two, so the join is commutative.
    #[must_use]
    pub fn join(mut self, other: Self) -> Self {
        for (at, line) in other.0 {
            match self.0.entry(at) {
                std::collections::btree_map::Entry::Vacant(vacant) => {
                    vacant.insert(line);
                }
                std::collections::btree_map::Entry::Occupied(mut held) => {
                    if line > *held.get() {
                        held.insert(line);
                    }
                }
            }
        }
        self
    }

    /// Each need's card, the earliest asked: one card per need, whoever asked for it.
    fn drawn(asking: &[Asking]) -> BTreeMap<&Need, &str> {
        let mut earliest: BTreeMap<&Need, &Asking> = BTreeMap::new();
        for card in asking {
            earliest
                .entry(&card.need)
                .and_modify(|held| *held = (*held).min(card))
                .or_insert(card);
        }
        earliest
            .into_iter()
            .map(|(need, card)| (need, card.card.as_str()))
            .collect()
    }

    /// Every group in [`Group::ALL`]'s order, each with its lines, `ask` naming where Ask
    /// opens. Every secret's name a line would show is put in words first, and each need's card
    /// is placed on the first line, in this order, that needs it: a partition of the cards.
    #[must_use]
    pub fn wire(self, names: &[&str], asking: &[Asking], ask: String) -> wire::Overview {
        let worded = Words::of(names);
        let mut cards = Self::drawn(asking);
        let mut lines: BTreeMap<Group, Vec<wire::OverviewLine>> = BTreeMap::new();
        for ((group, slot), line) in self.0 {
            let placed = line
                .needs
                .iter()
                .filter_map(|need| cards.remove(need).map(str::to_owned))
                .collect();
            lines
                .entry(group)
                .or_default()
                .push(worded.line(slot.key(), line, placed));
        }
        wire::Overview {
            groups: Group::ALL
                .into_iter()
                .map(|group| wire::OverviewGroup {
                    group,
                    lines: lines.remove(&group).unwrap_or_default(),
                })
                .collect(),
            ask,
        }
    }
}

/// Every secret's name, each with what the owner calls it, longest first so no name is replaced
/// inside a longer one.
struct Words(Vec<(String, String)>);

impl Words {
    fn of(names: &[&str]) -> Self {
        let mut pairs: Vec<(String, String)> = names
            .iter()
            .map(|name| ((*name).to_owned(), Help::label(name)))
            .collect();
        pairs.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        Self(pairs)
    }

    fn say(&self, text: &str) -> String {
        self.0.iter().fold(text.to_owned(), |text, (name, label)| {
            text.replace(name, label)
        })
    }

    fn line(&self, key: String, line: Line, cards: Vec<String>) -> wire::OverviewLine {
        wire::OverviewLine {
            key,
            label: self.say(&line.label),
            detail: line.detail.map(|detail| self.say(&detail)),
            standing: match line.standing {
                Standing::Failing {
                    since,
                    class,
                    words,
                    at,
                } => Standing::Failing {
                    since,
                    class,
                    words: self.say(&words),
                    at,
                },
                Standing::Unreadable { words } => Standing::Unreadable {
                    words: self.say(&words),
                },
                Standing::Missing { what } => Standing::Missing {
                    what: what.iter().map(|what| self.say(what)).collect(),
                },
                other => other,
            },
            cards,
        }
    }
}

/// The overview of `declared` as `observed` reads it.
#[must_use]
pub fn overview(declared: &Declared<'_>, observed: &Observed<'_>) -> Overview {
    let connections = connect::Declared {
        accounts: declared.accounts.to_vec(),
        plugins: declared.plugins.to_vec(),
        drive: declared.drive.to_vec(),
        channels: declared.channels.to_vec(),
    }
    .connections();
    let connected = Connected {
        connections: &connections,
        observed,
        reach: declared.reach,
    };
    [
        storage(declared, observed, &connected),
        mail(declared, observed, &connected),
        plugins(declared, observed),
        drive(declared, observed, &connected),
        models(declared, observed),
        notifications(declared, &connected),
        coding(declared, observed),
        this_box(declared),
    ]
    .into_iter()
    .fold(Overview::default(), Overview::join)
}

fn line(label: impl Into<String>, detail: Option<String>, standing: Standing) -> Line {
    Line {
        label: label.into(),
        detail,
        standing,
        needs: BTreeSet::new(),
    }
}

fn needing(mut line: Line, needs: impl IntoIterator<Item = Need>) -> Line {
    line.needs.extend(needs);
    line
}

/// The removal of the connection keyed `group`/`name`.
fn removal(group: Group, name: &str) -> Need {
    Need::Removal(Key {
        group,
        name: name.to_owned(),
    })
}

fn secret(name: &str) -> Need {
    Need::Secret(name.to_owned())
}

/// `a, b and c`.
fn listed(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The data repository: while it names nowhere to push to, its template's step to name one;
/// once it does, its pushes as the push's own record reads them.
fn storage(
    declared: &Declared<'_>,
    observed: &Observed<'_>,
    connected: &Connected<'_>,
) -> Overview {
    let storage = declared.storage;
    let chains = declared.deployment.chains.names();
    let as_read = || {
        Standing::of_health(
            observed
                .registers
                .pushes
                .map(crate::health::Health::reading),
        )
    };
    let (pushed, standing) = match &storage.pushed {
        Pushed::To(remote) => (format!(" and pushed to {}", remote.as_str()), as_read()),
        Pushed::Folder(_) => (" and pushed to a folder on this box".to_owned(), as_read()),
        Pushed::Nowhere => (
            " and pushed nowhere".to_owned(),
            connected.standing(
                Group::Storage,
                "data",
                Checked::of(observed.registers.pushes),
            ),
        ),
        Pushed::Unread(why) => (
            String::new(),
            Standing::Unreadable {
                words: format!("where it is pushed could not be read: {why}"),
            },
        ),
    };
    Overview::line(
        Group::Storage,
        Slot::Repository,
        line(
            "Data repository",
            Some(format!(
                "Kept on this box{pushed}; it holds the {} {}.",
                if chains.len() == 1 { "chain" } else { "chains" },
                listed(&chains)
            )),
            standing,
        ),
    )
}

fn mail(declared: &Declared<'_>, observed: &Observed<'_>, connected: &Connected<'_>) -> Overview {
    declared
        .accounts
        .iter()
        .map(|account| {
            let grants = observed
                .grants
                .get(account.email.as_str())
                .copied()
                .unwrap_or_default();
            let detail = match MailCredential::of(account, grants) {
                MailCredential::Slots { imap, smtp } => format!(
                    "Microsoft: reading {}, sending {}.",
                    granted(imap),
                    granted(smtp)
                ),
                MailCredential::Lieer => "Gmail, through lieer's own grant.".to_owned(),
                MailCredential::Google { granted: held } => format!(
                    "Google: mail and calendars by one sign-in with Google, {}.",
                    granted(held)
                ),
                MailCredential::Bridge => "Proton, through the local Bridge.".to_owned(),
            };
            let sources = connected
                .connection(Group::Mail, account.email.as_str())
                .map(Connection::sources)
                .unwrap_or_default();
            let (checked, read) = observed.read(&sources);
            Overview::line(
                Group::Mail,
                Slot::Account(account.email.to_string()),
                needing(
                    line(
                        account.email.as_str(),
                        Some(detail),
                        connected
                            .standing(Group::Mail, account.email.as_str(), checked)
                            .join(read),
                    ),
                    matches!(account.sync, crate::mail::account::Sync::Google)
                        .then_some(Need::GoogleClient)
                        .into_iter()
                        .chain([
                            Need::MailSignIn(account.email.to_string()),
                            Need::MailSending(account.email.to_string()),
                            removal(Group::Mail, account.email.as_str()),
                        ]),
                ),
            )
        })
        .fold(Overview::default(), Overview::join)
}

const fn granted(yes: bool) -> &'static str {
    if yes { "signed in" } else { "not signed in" }
}

/// One line per credential a plugin declares, each a calendar where its store is one the
/// calendar reads; a plugin that declares none is one line of its own. A store that cannot be
/// read says so on every line of its plugin.
fn plugins(declared: &Declared<'_>, observed: &Observed<'_>) -> Overview {
    declared
        .plugins
        .iter()
        .flat_map(|manifest| {
            let name = manifest.name();
            let (group, store) = match observed
                .registers
                .plugins
                .get(name)
                .cloned()
                .unwrap_or_default()
            {
                Store::Calendar => (Group::Calendar, Standing::Kept),
                Store::NotBegun | Store::Other => (Group::Plugins, Standing::Kept),
                Store::Unreadable(why) => (
                    Group::Plugins,
                    Standing::Unreadable {
                        words: format!("its store could not be read: {why}"),
                    },
                ),
            };
            let through = (group == Group::Plugins).then(|| format!("Through the {name} plugin."));
            let credentials: Vec<Overview> = manifest
                .credentials()
                .map(|credential| {
                    let (_, read) = observed.read(&[manifest.link(credential)]);
                    let standing = observed
                        .secret(credential.as_str())
                        .join(read)
                        .join(store.clone());
                    Overview::line(
                        group,
                        Slot::Credential(name.clone(), credential.label().to_lowercase()),
                        needing(
                            line(
                                connect::Label::of_suffix(credential.label()).map_or_else(
                                    |_| credential.label().to_owned(),
                                    |label| label.shown(),
                                ),
                                through.clone(),
                                standing,
                            ),
                            [secret(credential.as_str())]
                                .into_iter()
                                // Only a link not yet stored was refused for its origin.
                                .chain(
                                    (observed.secrets.get(credential.as_str())
                                        != Some(&SecretState::Set))
                                    .then(|| Need::Grant(name.clone())),
                                )
                                .chain(
                                    connect::Label::of_suffix(credential.label())
                                        .map(|label| removal(Group::Calendar, label.as_str())),
                                ),
                        ),
                    )
                })
                .collect();
            if credentials.is_empty() {
                let (_, ran) = observed.read(&[manifest.ran()]);
                let standing = ran.join(store);
                vec![Overview::line(
                    group,
                    Slot::Plugin(name.clone()),
                    line(name.as_str(), through, standing),
                )]
            } else {
                credentials
            }
        })
        .fold(Overview::default(), Overview::join)
}

/// One line for the Drive: every allowed folder's connection, joined, and its probes. While a
/// probe fails for a reason a sign-in does not mend, the line is that failure in words, and no
/// step of the template stands above it.
fn drive(declared: &Declared<'_>, observed: &Observed<'_>, connected: &Connected<'_>) -> Overview {
    if declared.drive.is_empty() {
        return Overview::default();
    }
    let sources: BTreeSet<SourceKey> = declared
        .drive
        .iter()
        .filter_map(|folder| connected.connection(Group::Drive, &folder.to_string()))
        .flat_map(Connection::sources)
        .collect();
    let probed = (sources.iter()).find_map(|key| observed.registers.health.health(key));
    let reading = probed.map(crate::health::Health::reading);
    // A sign-in that lapsed is the template's terminal step again, however often the Drive
    // listed before; a failure a sign-in does not mend shows no step.
    let steps = match &reading {
        None | Some(crate::health::Reading::Ok { .. }) => Some(Checked::of(probed)),
        Some(crate::health::Reading::Failing { class, .. }) => match class.remedy() {
            connect::Remedy::Auth => Some(Checked::Never),
            connect::Remedy::Secret | connect::Remedy::Retry => None,
        },
    };
    let as_probed = Standing::of_health(reading);
    let standing = match steps {
        Some(checked) => declared
            .drive
            .iter()
            .map(|folder| connected.standing(Group::Drive, &folder.to_string(), checked))
            .fold(as_probed, Standing::join),
        None => as_probed,
    };
    let folders = declared
        .drive
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Overview::line(
        Group::Drive,
        Slot::Drive,
        line(
            "Proton Drive",
            Some(format!("The secretary may read {folders}.")),
            standing,
        ),
    )
}

/// One line per job the deployment runs a model for: a job whose calls are recorded stands as
/// they answered, or, never called, as its credential is stored.
fn models(declared: &Declared<'_>, observed: &Observed<'_>) -> Overview {
    declared
        .jobs
        .iter()
        .map(|job| {
            let (standing, needs) = match &job.calls {
                // What a model's calls said outranks whether its key resolves, as the bootstrap
                // reads it: the model may hold a sign-in of its own that no store here names.
                Calls::Recorded { key, credential } => (
                    match observed.registers.answered.reading(key) {
                        Some(reading) => Standing::of_calls(Some(reading)),
                        None => observed.secret(credential).join(Standing::Unobserved),
                    },
                    vec![secret(credential), Need::SignIn, Need::SignInCode],
                ),
                Calls::Unrecorded => (Standing::Kept, Vec::new()),
            };
            Overview::line(
                Group::Models,
                Slot::Job(job.label.clone()),
                needing(
                    line(job.label.clone(), Some(job.detail.clone()), standing),
                    needs,
                ),
            )
        })
        .fold(Overview::default(), Overview::join)
}

fn notifications(declared: &Declared<'_>, connected: &Connected<'_>) -> Overview {
    declared
        .channels
        .iter()
        .map(|&channel| {
            let standing = connected.of(&Connection::of_channel(channel), Checked::Never);
            Overview::line(
                Group::Notifications,
                Slot::Channel(channel),
                needing(
                    line(channel.as_str(), None, standing),
                    crate::notify::channel_secrets(channel)
                        .into_iter()
                        .map(secret),
                ),
            )
        })
        .fold(Overview::default(), Overview::join)
}

fn coding(declared: &Declared<'_>, observed: &Observed<'_>) -> Overview {
    declared
        .deployment
        .coding
        .as_ref()
        .map_or_else(Overview::default, |coding| {
            let token = crate::coding::ROUTINE_TOKEN;
            Overview::line(
                Group::Coding,
                Slot::Routine,
                needing(
                    line(
                        "Coding sessions",
                        Some(format!(
                            "A tapped session fires the routine on {}.",
                            coding.repo
                        )),
                        observed.secret(token),
                    ),
                    [secret(token)],
                ),
            )
        })
}

fn this_box(declared: &Declared<'_>) -> Overview {
    let build = declared.build;
    Overview::line(
        Group::Box,
        Slot::Build,
        line(
            format!("Suzatary {}", build.version),
            Some(format!(
                "{}{}",
                if build.build == "dev" {
                    "A development build outside the flake.".to_owned()
                } else {
                    format!("Built from revision {}.", build.build)
                },
                completed(&build.completes)
            )),
            Standing::Kept,
        ),
    )
}

/// What a version completed, as a sentence after the build's, or nothing.
fn completed(completes: &[wire::Shipped]) -> String {
    let titles: Vec<&str> = completes
        .iter()
        .map(|c| {
            c.held
                .as_ref()
                .map_or(c.todo.as_str(), |held| held.title.as_str())
        })
        .collect();
    match titles.as_slice() {
        [] => String::new(),
        [one] => format!(" It completed {one}."),
        [rest @ .., last] => format!(" It completed {} and {last}.", rest.join(", ")),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::*;
    use crate::mail::account::{MailAccount, MsImap, Proton, Sync};
    use crate::mail::verbs::{AccountSync, SyncReport};

    /// A calendar of two links, `PLUGIN_ICS_HOME` and `PLUGIN_ICS_WORK`.
    pub(crate) fn calendar() -> Manifest {
        manifest("ics", &["HOME", "WORK"])
    }

    fn manifest(name: &str, links: &[&str]) -> Manifest {
        let credentials = links
            .iter()
            .map(|link| format!("\"PLUGIN_{}_{link}\"", name.to_uppercase()))
            .collect::<Vec<_>>()
            .join(", ");
        toml::from_str(&format!(
            "name = \"{name}\"\ncomponent = \"{name}.wasm\"\nschema = \"{name}.schema\"\n\
             credentials = [{credentials}]\n"
        ))
        .expect("a manifest")
    }

    /// One generated deployment and what its stores observed, every name it may hold drawn from
    /// small alphabets so tables collide and secrets repeat.
    #[derive(Debug, Clone)]
    struct World {
        jobs: Vec<ModelJob>,
        channels: Vec<crate::connect::Channel>,
        accounts: Vec<MailAccount>,
        plugins: Vec<Manifest>,
        coding: bool,
        drive: Vec<crate::drive::DrivePath>,
        secrets: BTreeMap<String, SecretState>,
        grants: BTreeMap<String, MailGrants>,
        failing: Vec<(String, String)>,
        seen: BTreeMap<PluginName, Store>,
        asking: Vec<Asking>,
        pushed: Pushed,
        links: Healths<SourceKey>,
        pushes: Option<crate::health::Health>,
    }

    impl World {
        /// A deployment with nothing declared, its data repository pushed nowhere.
        fn empty() -> Self {
            Self {
                jobs: Vec::new(),
                channels: Vec::new(),
                accounts: Vec::new(),
                plugins: Vec::new(),
                coding: false,
                drive: Vec::new(),
                secrets: BTreeMap::new(),
                grants: BTreeMap::new(),
                failing: Vec::new(),
                seen: BTreeMap::new(),
                asking: Vec::new(),
                pushed: Pushed::Nowhere,
                links: Healths::default(),
                pushes: None,
            }
        }

        fn deployment(&self) -> Deployment {
            let actor = |name: &str| prodrome_core::event::Actor::new(name).expect("an actor");
            let chain = crate::chains::ChainPolicy {
                name: crate::chains::ChainName::new("life").expect("a slug"),
                untrusted: prodrome_core::policy::Untrusted::of([actor("triage")]),
                roster: [actor("ada"), actor("triage")].into_iter().collect(),
                root: crate::cast::keys::root(),
            };
            Deployment {
                viewers: BTreeSet::new(),
                actors: BTreeMap::new(),
                chains: crate::chains::Chains::of(vec![chain]).expect("one chain"),
                categories: BTreeMap::new(),
                app_url: "https://example.test".to_owned(),
                coding: self.coding(),
                zone: crate::zone::Zone::UTC,
            }
        }

        fn coding(&self) -> Option<crate::coding::Coding> {
            self.coding.then(|| crate::coding::Coding {
                routine: "trig01fixture".parse().expect("a routine id"),
                repo: "example/repo".parse().expect("a repository"),
            })
        }

        fn settables(&self) -> crate::settings::Settables {
            crate::settings::Settables::derive(
                crate::settings::MODEL_TOKEN,
                self.channels.iter().copied(),
                self.coding().as_ref(),
                &self.plugins,
            )
        }

        fn mail(&self) -> Healths<SourceKey> {
            SyncReport {
                unheld: None,
                synced: self
                    .failing
                    .iter()
                    .map(|(account, tail)| AccountSync {
                        account: crate::mail::account_email::AccountEmail::new(account)
                            .expect("a generated address"),
                        outcome: crate::health::Outcome::Failed(connect::Failure::Unclassified),
                        tail: tail.clone(),
                    })
                    .collect(),
                notmuch: String::new(),
            }
            .healths(
                chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
                    .and_then(|day| day.and_hms_opt(9, 0, 0))
                    .expect("an instant"),
            )
        }

        /// The overview, and every name the deployment declares.
        fn read(&self) -> (Overview, Vec<String>) {
            let deployment = self.deployment();
            let settables = self.settables();
            let build = wire::AboutPayload {
                version: "0.0.0".to_owned(),
                build: "dev".to_owned(),
                completes: Vec::new(),
            };
            let declared = Declared {
                deployment: &deployment,
                settables: &settables,
                channels: &self.channels,
                accounts: &self.accounts,
                jobs: &self.jobs,
                plugins: &self.plugins,
                drive: &self.drive,
                storage: &Storage {
                    root: "/srv/data".to_owned(),
                    pushed: self.pushed.clone(),
                },
                build: &build,
                reach: &crate::connect::PLAIN,
            };
            let health = self.mail().join(&self.links);
            let observed = Observed {
                secrets: &self.secrets,
                grants: &self.grants,
                registers: Registers {
                    health: &health,
                    answered: &Healths::default(),
                    plugins: &self.seen,
                    pushes: self.pushes.as_ref(),
                },
            };
            let names = settables.names().into_iter().map(str::to_owned).collect();
            (overview(&declared, &observed), names)
        }

        fn wire(&self) -> (wire::Overview, Vec<String>) {
            let (overview, names) = self.read();
            let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
            (
                overview.wire(&borrowed, &self.asking, "setup".to_owned()),
                names,
            )
        }
    }

    fn account(email: &str, kind: u8) -> MailAccount {
        let sync = match kind {
            0 => Sync::Gmail,
            1 => Sync::MsImap(MsImap {
                authority: crate::mail::account::Authority::Organizations,
                inbox_folders: crate::mail::folders::Folders::inbox(),
                sent_folder: "Sent Items".to_owned(),
            }),
            3 => Sync::Google,
            _ => Sync::Proton(Proton {
                channel: "proton-all".to_owned(),
                smtp_user: email.to_owned(),
                send_addresses: vec![email.to_owned()],
                inbox_folders: crate::mail::folders::Folders::inbox(),
            }),
        };
        MailAccount {
            subdir: email.replace(['@', '.', '+'], "-"),
            email: crate::mail::account_email::AccountEmail::new(email)
                .expect("a lower-case address"),
            sync,
        }
    }

    fn state() -> impl Strategy<Value = SecretState> {
        prop_oneof![
            Just(SecretState::Set),
            Just(SecretState::Missing),
            Just(SecretState::Locked),
            Just(SecretState::Failed),
        ]
    }

    /// The jobs a deployment may run a model for: one whose calls are recorded, one not.
    fn jobs() -> Vec<ModelJob> {
        vec![
            ModelJob {
                label: "Chat".to_owned(),
                detail: "It runs a-model.".to_owned(),
                calls: Calls::Recorded {
                    key: "a-model".to_owned(),
                    credential: crate::settings::MODEL_TOKEN.to_owned(),
                },
            },
            ModelJob {
                label: "Compactor".to_owned(),
                detail: "Off.".to_owned(),
                calls: Calls::Unrecorded,
            },
        ]
    }

    fn world() -> impl Strategy<Value = World> {
        let jobs = proptest::sample::subsequence(jobs(), 0..=2);
        let channels = proptest::sample::subsequence(connect::Channel::ALL.to_vec(), 0..=2);
        let accounts =
            proptest::collection::btree_map("ada(\\+[a-c])?@example\\.test", 0u8..4, 0..4);
        let plugins = proptest::collection::btree_map(
            proptest::sample::select(vec!["ics", "due", "rota"]),
            (
                proptest::sample::subsequence(vec!["HOME", "WORK", "TOKEN"], 0..=3),
                0u8..4,
                proptest::option::of(0u8..4),
            ),
            0..3,
        );
        (
            jobs,
            channels,
            accounts,
            plugins,
            any::<bool>(),
            any::<bool>(),
            proptest::collection::vec(state(), 16),
            proptest::collection::vec(any::<(bool, bool, bool)>(), 4),
            proptest::collection::vec((0u8..4, 0usize..8, 1u8..10), 0..6),
        )
            .prop_map(
                |(jobs, channels, accounts, plugins, coding, drive, states, flags, cards)| {
                    let manifests: Vec<Manifest> = plugins
                        .iter()
                        .map(|(name, (links, _, _))| manifest(name, links))
                        .collect();
                    let mut world = World {
                        jobs,
                        channels,
                        accounts: accounts
                            .iter()
                            .map(|(email, kind)| account(email, *kind))
                            .collect(),
                        plugins: manifests,
                        coding,
                        drive: if drive {
                            vec!["/my-files/Docs".parse().expect("a path")]
                        } else {
                            Vec::new()
                        },
                        secrets: BTreeMap::new(),
                        grants: BTreeMap::new(),
                        failing: Vec::new(),
                        seen: BTreeMap::new(),
                        asking: Vec::new(),
                        pushed: Pushed::Nowhere,
                        links: Healths::default(),
                        pushes: None,
                    };
                    let names: Vec<String> = world
                        .settables()
                        .names()
                        .into_iter()
                        .map(str::to_owned)
                        .collect();
                    world.secrets = names
                        .iter()
                        .zip(states.iter().cycle())
                        .map(|(name, state)| (name.clone(), *state))
                        .collect();
                    for ((account, _), (imap, smtp, failing)) in accounts.iter().zip(&flags) {
                        world.grants.insert(
                            account.clone(),
                            MailGrants {
                                imap: *imap,
                                smtp: *smtp,
                            },
                        );
                        if *failing {
                            // The words a sync says may name a secret; the page must not.
                            let said = names.first().cloned().unwrap_or_default();
                            world
                                .failing
                                .push((account.clone(), format!("{said} was refused")));
                        }
                    }
                    for (name, (links, store, said)) in plugins {
                        let (plugin, seen, fetched) = seen(name, &links, store, said);
                        world.seen.insert(plugin, seen);
                        world.links = world.links.join(&fetched);
                    }
                    let emails: Vec<String> = accounts.keys().cloned().collect();
                    world.asking = asking(&cards, &names, &emails);
                    world
                },
            )
    }

    /// The open cards `cards` picks: each a secret of `names`, an account of `emails` or the
    /// sign-in, asked on its day.
    fn asking(cards: &[(u8, usize, u8)], names: &[String], emails: &[String]) -> Vec<Asking> {
        cards
            .iter()
            .enumerate()
            .filter_map(|(n, (kind, pick, day))| {
                let need = match kind {
                    0 => Need::Secret(names.get(pick % names.len().max(1))?.clone()),
                    1 => Need::MailSignIn(emails.get(pick % emails.len().max(1))?.clone()),
                    2 => Need::SignIn,
                    _ => Need::SignInCode,
                };
                Some(Asking {
                    need,
                    at: format!("2026-10-0{day}T09:00:00"),
                    card: format!("proposal:c/{n}"),
                })
            })
            .collect()
    }

    /// A plugin's store and its links' health: every link last fetched as `said` picks (never,
    /// 200, 401, a refusal naming the link).
    fn seen(
        name: &str,
        links: &[&str],
        store: u8,
        said: Option<u8>,
    ) -> (PluginName, Store, Healths<SourceKey>) {
        let plugin = PluginName::parse(name).expect("a name");
        let mut fetched = Healths::default();
        for link in links {
            let credential = format!("{}{link}", plugin.credential_prefix());
            let answer = match said {
                None => continue,
                Some(0) => crate::plugin::Answer::Status(200),
                Some(1) => crate::plugin::Answer::Status(401),
                Some(2) => crate::plugin::Answer::Refused(crate::plugin::Refusal::CredentialUnset(
                    crate::plugin::CredentialName::parse(&plugin, &credential).expect("a name"),
                )),
                Some(_) => break,
            };
            let tried = chrono::NaiveDate::from_ymd_opt(2026, 10, 5)
                .and_then(|day| day.and_hms_opt(10, 0, 0))
                .expect("an instant");
            let run = crate::plugin::LastRun {
                at: tried,
                came: crate::plugin::Came::Learnt,
            };
            let credential =
                crate::plugin::CredentialName::parse(&plugin, &credential).expect("a name");
            fetched = fetched.join(&run.health(&manifest(name, links), &[(credential, answer)]));
        }
        let store = match store {
            0 => Store::NotBegun,
            1 => Store::Calendar,
            2 => Store::Other,
            _ => Store::Unreadable(format!("{} holds no schema", plugin.credential_prefix())),
        };
        (plugin, store, fetched)
    }

    fn standing() -> impl Strategy<Value = Standing> {
        prop_oneof![
            Just(Standing::Kept),
            "2026-10-0[1-3]".prop_map(|at| Standing::Synced { at }),
            "2026-10-0[1-3]".prop_map(|at| Standing::Answered { at }),
            Just(Standing::Unobserved),
            Just(Standing::Locked),
            Just(Standing::StoreFailed),
            "[a-c]{1,3}".prop_map(|words| Standing::Unreadable { words }),
            (0usize..3, "[a-c]{1,3}", "[a-c]{1,3}")
                .prop_map(|(step, command, why)| { Standing::ByHand { step, command, why } }),
            (
                "2026-10-0[1-3]",
                proptest::sample::select(connect::Failure::ALL.to_vec()),
                "[a-c]{1,3}"
            )
                .prop_map(|(since, class, words)| Standing::Failing {
                    at: since.clone(),
                    since,
                    class,
                    words,
                }),
            proptest::collection::btree_set("[a-c]", 1..3).prop_map(|what| Standing::Missing {
                what: what.into_iter().collect()
            }),
        ]
    }

    fn lines(overview: &wire::Overview, group: Group) -> usize {
        overview
            .groups
            .iter()
            .find(|held| held.group == group)
            .map_or(0, |held| held.lines.len())
    }

    proptest! {
        /// A line's parts join as a semilattice whose identity is `Kept`.
        #[test]
        fn standing_joins_as_a_semilattice(a in standing(), b in standing(), c in standing()) {
            prop_assert_eq!(a.clone().join(b.clone()), b.clone().join(a.clone()));
            prop_assert_eq!(
                a.clone().join(b.clone()).join(c.clone()),
                a.clone().join(b.clone().join(c))
            );
            prop_assert_eq!(a.clone().join(a.clone()), a.clone());
            prop_assert_eq!(a.clone().join(Standing::Kept), a);
        }

        /// The overview is a function of the set of declarations: writing the tables in another
        /// order reads the same.
        #[test]
        fn the_overview_is_a_function_of_the_instance_and_stores(world in world(), seed in any::<u64>()) {
            let mut shuffled = world.clone();
            let turn = |n: usize| usize::try_from(seed % 7).unwrap_or(0) % n.max(1);
            let by = turn(shuffled.jobs.len());
            shuffled.jobs.rotate_left(by);
            shuffled.channels.reverse();
            let by = turn(shuffled.accounts.len());
            shuffled.accounts.rotate_left(by);
            shuffled.plugins.reverse();
            shuffled.asking.reverse();
            prop_assert_eq!(world.read().0, shuffled.read().0);
            prop_assert_eq!(world.wire().0, shuffled.wire().0);
        }

        /// The tables of two halves of a deployment read as the join of what each reads.
        #[test]
        fn reading_is_a_homomorphism_over_the_tables(world in world(), cut in 0usize..4) {
            let half = |keep: &dyn Fn(usize) -> bool| {
                let mut half = world.clone();
                half.accounts = world.accounts.iter().enumerate().filter(|(i, _)| keep(*i)).map(|(_, a)| a.clone()).collect();
                half.channels = world.channels.iter().enumerate().filter(|(i, _)| keep(*i)).map(|(_, c)| *c).collect();
                half.plugins = world.plugins.iter().enumerate().filter(|(i, _)| keep(*i)).map(|(_, p)| p.clone()).collect();
                half
            };
            let left = half(&|i| i < cut);
            let right = half(&|i| i >= cut);
            prop_assert_eq!(world.read().0, left.read().0.join(right.read().0));
        }

        /// A group nothing is connected to lists nothing, and one something is connected to
        /// lists it.
        #[test]
        fn a_group_with_nothing_connected_lists_nothing(world in world()) {
            let (page, _) = world.wire();
            let calendars = world.plugins.iter().filter(|m| world.seen.get(m.name()) == Some(&Store::Calendar)).count();
            prop_assert_eq!(lines(&page, Group::Mail), world.accounts.len());
            prop_assert_eq!(lines(&page, Group::Notifications), world.channels.len());
            prop_assert_eq!(lines(&page, Group::Models), world.jobs.len());
            prop_assert_eq!(lines(&page, Group::Coding) > 0, world.coding);
            prop_assert_eq!(lines(&page, Group::Drive) > 0, !world.drive.is_empty());
            prop_assert_eq!(lines(&page, Group::Calendar) > 0, calendars > 0);
            prop_assert_eq!(lines(&page, Group::Plugins) > 0, world.plugins.len() > calendars);
            prop_assert_eq!(lines(&page, Group::Storage), 1);
            prop_assert_eq!(lines(&page, Group::Box), 1);
        }

        /// No secret's stored name is on the wire, whatever a sync, a fetch or a store said.
        #[test]
        fn no_key_name_is_on_the_wire(world in world()) {
            let (page, names) = world.wire();
            let json = serde_json::to_string(&page).expect("serialises");
            for name in &names {
                prop_assert!(!json.contains(name.as_str()), "{} is on the wire: {}", name, json);
            }
        }

        /// Each need some line has is drawn once, by its earliest card, on the first line that
        /// needs it; a need no line has is drawn nowhere.
        #[test]
        fn each_need_is_drawn_once_on_its_first_line(world in world()) {
            let (page, _) = world.wire();
            let (overview, _) = world.read();
            let placed: Vec<(&str, &str)> = page.groups.iter()
                .flat_map(|group| group.lines.iter().flat_map(|line| line.cards.iter().map(move |card| (line.key.as_str(), card.as_str()))))
                .collect();
            let mut wanted: Vec<(String, &str)> = Vec::new();
            for ((_, slot), line) in &overview.0 {
                for need in &line.needs {
                    let earliest = world.asking.iter().filter(|card| &card.need == need).min();
                    if let Some(card) = earliest
                        && !wanted.iter().any(|(_, drawn)| *drawn == card.card)
                    {
                        wanted.push((slot.key(), card.card.as_str()));
                    }
                }
            }
            let mut placed_sorted: Vec<(String, &str)> = placed.iter().map(|(key, card)| ((*key).to_owned(), *card)).collect();
            placed_sorted.sort();
            wanted.sort();
            prop_assert_eq!(placed_sorted, wanted);
        }

        /// A draft waiting to be sent from an account never hides its sign-in: whichever was
        /// asked first, the account's line draws the sign-in's card, and the draft's nowhere.
        #[test]
        fn a_waiting_draft_never_hides_the_accounts_sign_in(draft_first in any::<bool>(), kind in 0u8..4) {
            use crate::secretary::inbox;
            use crate::secretary::store::{ConversationId, Draft, Reasons};
            let email = "ada@example.test";
            let store = crate::store::InMemory::<crate::secretary::inbox_event::InboxEvent>::default();
            let ask = |index: usize, proposal: Proposal, minute: u32| {
                let asked = inbox::Asked {
                    proposer: crate::secretary::proposer::Proposer::Secretary,
                    proposal,
                    reasons: Reasons::new("asked", vec!["briefing:2026-10-07".parse().expect("a reference")]).expect("reasons"),
                    asked: crate::reference::ProposalRef {
                        conversation: ConversationId::new("setup").expect("a slug"),
                        index,
                    },
                    price: inbox::Price::unpriced(),
                };
                let at = crate::capability::Datetime::new(2026, 10, 7, 9, minute, 0, 0).expect("an instant");
                inbox::ask(&store, asked, &crate::secretary::writes_as(), at).expect("filed");
            };
            let (draft, sign_in) = if draft_first { (0, 1) } else { (1, 0) };
            ask(0, Proposal::Email(Draft { account: email.to_owned(), ..Draft::default() }), draft);
            ask(1, Proposal::Act(Act::Auth {
                account: crate::mail::address::EmailAddress::new(email).expect("an address"),
            }), sign_in);
            let mut world = World {
                accounts: vec![account(email, kind)],
                ..World::empty()
            };
            world.asking = Asking::open(&inbox::fold(&store).expect("reads"));
            let (page, _) = world.wire();
            let mail = page.groups.iter().find(|group| group.group == Group::Mail).expect("a mail group");
            prop_assert_eq!(&mail.lines[0].cards, &vec!["proposal:setup/1".to_owned()]);
        }
    }

    /// Where the data repository is pushed is said only as read: an unread configuration asks
    /// to be looked at, never reads as "pushed nowhere".
    #[test]
    fn the_data_repository_says_where_it_is_pushed_only_as_read() {
        let said = |pushed: Pushed| {
            let (overview, _) = World {
                pushed,
                ..World::empty()
            }
            .read();
            let line = overview.0[&(Group::Storage, Slot::Repository)].clone();
            (line.detail.unwrap_or_default(), line.standing)
        };
        let remote = crate::settings::remote::Remote::parse("git@forge.example.test:ada/data.git")
            .expect("a remote");
        assert_eq!(
            said(Pushed::To(remote)),
            (
                "Kept on this box and pushed to forge.example.test/ada/data; it holds the chain life."
                    .to_owned(),
                Standing::Unobserved
            )
        );
        assert_eq!(
            said(Pushed::Folder("/srv/git/data.git".to_owned())).0,
            "Kept on this box and pushed to a folder on this box; it holds the chain life."
        );
        let (detail, standing) = said(Pushed::Nowhere);
        assert_eq!(
            detail,
            "Kept on this box and pushed nowhere; it holds the chain life."
        );
        assert!(
            matches!(&standing, Standing::ByHand { command, .. } if command.contains("remote add origin")),
            "a repository pushed nowhere asks for its template's step: {standing:?}"
        );
        let (detail, standing) = said(Pushed::Unread("permission denied".to_owned()));
        assert!(!detail.contains("nowhere"), "{detail}");
        assert_eq!(
            standing,
            Standing::Unreadable {
                words: "where it is pushed could not be read: permission denied".to_owned()
            }
        );
    }
}
