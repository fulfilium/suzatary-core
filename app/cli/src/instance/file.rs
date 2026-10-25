//! `instance.toml` as written: each value parsed to its type as it is read,
//! each path as the file spells it until [`super::Instance::read`] roots it.
//!
//! `init` and the e2e fixture write the file through the same types, so the
//! file is written and read by one shape.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use prodrome_core::event::Actor;
use prodrome_core::sign::PublicKey;
use serde::{Deserialize, Serialize};
use suzatary_core::capability::Login;
use suzatary_core::chains::ChainName;
use suzatary_core::coding::Coding;
use suzatary_core::drive::DrivePath;
use suzatary_core::mail::account::Roster;
use suzatary_core::mail::junk::ChurnList;
use suzatary_core::model_name::ModelName;
use suzatary_core::persona::Persona;
use suzatary_core::plugin::Manifest;
use suzatary_core::repos::Declared;
use suzatary_core::web::AllowRule;

use super::{CodeRoot, DataRoot, Reserve};

/// The whole file. A key the binary does not read refuses the file, so a table left behind
/// is never a setting that sets nothing.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct File {
    pub(crate) repo: PathBuf,
    /// Absent, the data root is the instance file's own directory.
    #[serde(default = "here")]
    pub(crate) data: PathBuf,
    pub(crate) chains: Vec<Chain>,
    pub(crate) web: Web,
    pub(crate) categories: BTreeMap<String, u32>,
    pub(crate) mail: Mail,
    #[serde(default)]
    pub(crate) secretary: Secretary,
    #[serde(default, skip_serializing_if = "Triage::is_default")]
    pub(crate) agent: Triage,
    #[serde(default)]
    pub(crate) persona: Persona,
    /// The intent to notify, declared rather than inferred from which push
    /// secret resolves; TOML has no null, so "never" is the table absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) notify: Option<Notify>,
    /// Absent, a tap on a coding session is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) coding: Option<Coding>,
    /// The cache tier's root, under the data root: box-local, never pushed, rebuilt by syncing
    /// again (`cache` when absent). The data repository ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cache: Option<PathBuf>,
    /// Every plugin this box runs, each parsed once into its authorities. Never serialised:
    /// `init` declares none, and a calendar's tap writes its table through `toml_edit`
    /// (`connections.rs`), keeping every other byte.
    #[serde(default, skip_serializing)]
    pub(crate) plugins: Vec<Manifest>,
    /// The deployment's root (SPEC law 42), which every deployment declares: `suzatary init`
    /// mints the box's key and writes it.
    pub(crate) signatures: Signatures,
}

/// `[signatures]`: the root key, public, whose signatures make a device key count. Its seed is
/// the box's own key, `SUZATARY_BOX_KEY` in secretspec, never written here.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Signatures {
    #[serde(with = "text")]
    pub(crate) root: PublicKey,
}

fn here() -> PathBuf {
    PathBuf::from(".")
}

impl File {
    /// The file at `path`, each value parsed to its type: what [`super::Instance::read`] roots
    /// and checks, and what `suzatary instance` answers against.
    ///
    /// # Errors
    /// The file unreadable, or not an instance file.
    pub(crate) fn read(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading the instance file {}", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("parsing the instance file {}", path.display()))
    }

    /// Serialise, with a fixed header ahead of the tables.
    ///
    /// # Errors
    /// The value will not serialise.
    pub(crate) fn to_toml_string(&self, header: &str) -> anyhow::Result<String> {
        let body = toml::to_string_pretty(self).context("serialising the instance file")?;
        Ok(format!("{}\n{body}", header.trim_end()))
    }

    /// Write it to `path`, refusing to clobber an existing file unless `force`.
    ///
    /// # Errors
    /// `path` exists and `force` is false, or the write fails.
    pub(crate) fn write(&self, path: &Path, header: &str, force: bool) -> anyhow::Result<()> {
        if !force && path.exists() {
            bail!(
                "{} already exists — pass --force to overwrite it",
                path.display()
            );
        }
        let text = self.to_toml_string(header)?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }
}

/// One `[[chains]]` entry; the entries' order is meaning, the first being what
/// a verb with no `--chain` and the bare `/api/…` paths serve.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Chain {
    pub(crate) name: ChainName,
    pub(crate) events: PathBuf,
    #[serde(with = "texts")]
    pub(crate) untrusted: Vec<Actor>,
    #[serde(with = "texts")]
    pub(crate) actors: Vec<Actor>,
    /// The actor of the roster whose deployment this is: whom a command with no `--actor` acts
    /// as. Trusted and on the roster, or the file does not read.
    #[serde(with = "text")]
    pub(crate) owner: Actor,
}

// `toml` serialises every plain value ahead of a table, so `act_as` and
// `serving` come last.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Web {
    #[serde(with = "texts")]
    pub(crate) viewers: Vec<Login>,
    pub(crate) app_url: String,
    #[serde(with = "text_map")]
    pub(crate) act_as: BTreeMap<Login, Actor>,
    pub(crate) serving: Serving,
}

/// Where this deployment serves from and what it serves.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Serving {
    /// The address to bind.
    pub host: String,
    /// The port.
    pub port: u16,
    /// Where `tailscale cert` puts `cert.pem` and `key.pem`.
    pub tls_dir: PathBuf,
    /// The frontend build, the one directory served as files.
    pub dist: PathBuf,
    /// The briefings.
    pub briefings: PathBuf,
}

impl Serving {
    pub(super) fn rooted(self, code: &CodeRoot, data: &DataRoot) -> Self {
        Self {
            tls_dir: data.under(&self.tls_dir),
            dist: code.under(&self.dist),
            briefings: data.under(&self.briefings),
            ..self
        }
    }
}

/// `[mail]`: where the mail and its bookkeeping live, and whose it is.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Mail {
    /// The maildir root.
    pub root: PathBuf,
    /// The triage state file, the sole dedup authority.
    pub state: PathBuf,
    /// The notmuch binary.
    pub notmuch: PathBuf,
    /// Absent means no mail index, which the mail capability reads as a
    /// refusal rather than an ambient fallback.
    #[serde(default)]
    pub notmuch_config: Option<PathBuf>,
    /// The mbsync configuration the Proton channel is pinned to.
    pub mbsyncrc: PathBuf,
    /// A bare timestamp of the last triage pass that pushed, kept off the chain
    /// because the chain goes silent exactly when a pass cannot commit.
    pub heartbeat: PathBuf,
    /// The account roster, checked for unique addresses and maildir folders.
    pub accounts: Roster,
    /// The churn and keep lists, before any maintained seed is folded in.
    #[serde(default, skip_serializing_if = "junk::is_empty", with = "junk")]
    pub junk: ChurnList,
}

impl Mail {
    pub(super) fn rooted(self, data: &DataRoot) -> Self {
        Self {
            root: data.under(&self.root),
            state: data.under(&self.state),
            notmuch_config: self.notmuch_config.map(|named| data.under(&named)),
            mbsyncrc: data.under(&self.mbsyncrc),
            heartbeat: data.under(&self.heartbeat),
            ..self
        }
    }
}

/// `[agent]`, defaulted key by key: what the triage agent is handed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Triage {
    /// What its `context` may spend on the secretary's memory beyond the manual's cap: the memory,
    /// the newest facts first and then the record in the owner's words alone, gets this and what
    /// the manual leaves of its cap.
    pub memory_bytes: usize,
}

impl Triage {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for Triage {
    fn default() -> Self {
        Self {
            memory_bytes: 16 * 1024,
        }
    }
}

/// `[secretary]`, defaulted key by key: the secretary is never absent, because
/// a route that 404s on one deployment is a frontend that has to ask.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Secretary {
    /// The secretary's directory: the memory and the files a conversation holds; the
    /// conversations themselves are [`Secretary::line_store`], beside it.
    pub conversations: PathBuf,
    /// The cache of rendered answer blocks, a sibling of `conversations`
    /// because the transcript is append-only bytes nothing else may touch.
    pub renders: PathBuf,
    /// The inbox's store: every proposal and what became of it.
    pub inbox: PathBuf,
    /// The memory's history: which facts were remembered and which forgotten, by id; their words
    /// are [`Secretary::memory_texts`].
    pub memory: PathBuf,
    /// The digest's history: which nodes of the conversations' summary tree were summarised, by
    /// the node's name; their words are [`Secretary::digest_texts`].
    pub digests: PathBuf,
    /// The two topic stores, box-local, vaulted and never pushed.
    pub topics: Topics,
    /// The Claude model the conversation, the reviews and the titles run, pinned because a
    /// drifting default is a behaviour change nobody committed.
    pub model: ModelName,
    /// Whether the compactor summarises the conversations into the digest: off unless the file
    /// says `true`, and `false` is its kill switch. The five keys after it are read only when
    /// it runs ([`Secretary::compacting`]).
    pub compact: bool,
    /// The Claude model the compactor runs, over the subscription.
    pub compactor: ModelName,
    /// The share of the subscription the compactor never takes.
    pub reserve: Reserve,
    /// The most calls the compactor makes in any hour.
    pub compact_calls_per_hour: u32,
    /// The most bytes it sends in any twenty-four hours.
    pub compact_bytes_per_day: u64,
    /// How many of its steps may run at once, each on a node no other step holds.
    pub compact_workers: std::num::NonZeroUsize,
    /// The settings file that fences the model's subprocess, under the code root: always one,
    /// the deny-all this repository ships unless the file names another.
    pub settings: PathBuf,
    /// The hosts it may read without asking; "Always allow" appends here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub web_allow: Vec<AllowRule>,
    /// The Drive folders it reads without asking; "Always allow this folder"
    /// appends here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub drive_allow: Vec<DrivePath>,
    /// The Proton Drive CLI; `$HOME/bin/proton-drive` when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive_cli: Option<PathBuf>,
    /// The repositories the secretary may reach, each with its check; one not named here (or
    /// as `[coding] repo`) is refused before anything is asked.
    #[serde(skip_serializing_if = "is_none_declared", with = "repos")]
    pub repos: Declared,
}

impl Default for Secretary {
    fn default() -> Self {
        Self {
            conversations: PathBuf::from("secretary"),
            renders: PathBuf::from("secretary-render"),
            inbox: PathBuf::from("inbox"),
            memory: PathBuf::from("memory"),
            digests: PathBuf::from("digests"),
            topics: Topics::default(),
            model: ModelName::parse("claude-opus-5-5").expect("a model name"),
            // Off until the owner turns it on: the first run compacts every line already stored.
            compact: false,
            // The latest Sonnet, the owner's choice for the compactor (2026-10-06).
            compactor: ModelName::parse("claude-sonnet-5-5").expect("a model name"),
            reserve: Reserve::default(),
            // Thirty calls an hour and 32 MiB a day: an import of every past line takes days,
            // never an afternoon of the subscription.
            compact_calls_per_hour: 30,
            compact_bytes_per_day: 32 * 1024 * 1024,
            compact_workers: std::num::NonZeroUsize::MIN,
            settings: PathBuf::from(".claude/secretary-settings.json"),
            web_allow: Vec::new(),
            drive_allow: Vec::new(),
            drive_cli: None,
            repos: Declared::default(),
        }
    }
}

impl Secretary {
    /// The compactor as these keys set it: off, or a model within a budget.
    #[must_use]
    pub fn compacting(&self) -> suzatary_core::secretary::compacting::Compacting {
        use suzatary_core::secretary::compacting::Compacting;
        if !self.compact {
            return Compacting::Off;
        }
        Compacting::On {
            model: self.compactor.clone(),
            budget: suzatary_core::secretary::compact_budget::Budget {
                calls_per_hour: self.compact_calls_per_hour,
                bytes_per_day: self.compact_bytes_per_day,
            },
            workers: self.compact_workers,
        }
    }

    /// The conversations' lines, pushed with their words locked: `conversations` with `-lines`
    /// on its name, beside it.
    #[must_use]
    pub fn line_store(&self) -> PathBuf {
        self.beside("-lines")
    }

    /// The keys of the lines' words, box-local, vaulted and never pushed: `conversations` with
    /// `-lines-keys` on its name, beside it.
    #[must_use]
    pub fn line_keys(&self) -> PathBuf {
        self.beside("-lines-keys")
    }

    /// The conversations, their lines and their keys, emptying `renders` of what an erasure
    /// takes.
    #[must_use]
    pub fn conversation_stores(
        &self,
        renders: std::sync::Arc<dyn suzatary_core::capability::RenderCache + Send + Sync>,
    ) -> suzatary_io::conversations::ConversationStores {
        suzatary_io::conversations::ConversationStores::open(
            self.line_store(),
            self.line_keys(),
            renders,
        )
    }

    /// The memory's texts, box-local and never pushed: `conversations` with `-memory-texts` on
    /// its name, beside it.
    #[must_use]
    pub fn memory_texts(&self) -> PathBuf {
        self.beside("-memory-texts")
    }

    /// The digest's summaries' words, box-local and never pushed: `conversations` with
    /// `-digest-texts` on its name, beside it.
    #[must_use]
    pub fn digest_texts(&self) -> PathBuf {
        self.beside("-digest-texts")
    }

    /// The owner's uploads: `attachments/` inside the conversations directory, box-local and
    /// git-ignored with them.
    #[must_use]
    pub fn uploads(&self) -> PathBuf {
        self.conversations.join("attachments")
    }

    /// The files verbs hand the model: `files/` inside the conversations directory, beside the
    /// uploads and git-ignored with them.
    #[must_use]
    pub fn kept(&self) -> PathBuf {
        self.conversations.join("files")
    }

    /// The secretary's directory with `suffix` on its name, beside it.
    fn beside(&self, suffix: &str) -> PathBuf {
        let mut name = self
            .conversations
            .file_name()
            .unwrap_or_default()
            .to_os_string();
        name.push(suffix);
        self.conversations.with_file_name(name)
    }

    pub(super) fn rooted(self, code: &CodeRoot, data: &DataRoot) -> Self {
        Self {
            conversations: data.under(&self.conversations),
            renders: data.under(&self.renders),
            inbox: data.under(&self.inbox),
            memory: data.under(&self.memory),
            digests: data.under(&self.digests),
            topics: Topics {
                memberships: data.under(&self.topics.memberships),
                aliases: data.under(&self.topics.aliases),
            },
            settings: code.under(&self.settings),
            drive_cli: self.drive_cli.map(|named| data.under(&named)),
            ..self
        }
    }
}

/// `[secretary] topics`: where the topics and their aliases are kept. A topic name is words, so
/// both stores stay on the box: the vault carries them and the data push refuses them.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Topics {
    /// Which topics each round and note carries.
    pub memberships: PathBuf,
    /// Which topic names stand for another.
    pub aliases: PathBuf,
}

impl Default for Topics {
    fn default() -> Self {
        Self {
            memberships: PathBuf::from("secretary-topics"),
            aliases: PathBuf::from("secretary-topic-aliases"),
        }
    }
}

impl Topics {
    /// Both stores, opened.
    #[must_use]
    pub fn open(&self) -> suzatary_io::topics::Topics {
        suzatary_io::topics::Topics::open(self.memberships.clone(), self.aliases.clone())
    }
}

/// `[notify]`: a non-empty `channels` means push secrets are resolved at
/// startup; `channels` is a checklist for a human, since an unset secret
/// already resolves to nothing.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Notify {
    #[serde(default)]
    pub(crate) channels: Vec<suzatary_core::connect::Channel>,
    /// The Firebase project's number the box hands the Android portal at pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fcm_sender: Option<suzatary_core::portal::Sender>,
}

/// A value the file spells as one string, read through its own constructor.
pub(crate) trait Text: Sized {
    fn read(text: String) -> Result<Self, String>;
    fn spelled(&self) -> &str;
}

impl Text for Actor {
    fn read(text: String) -> Result<Self, String> {
        Actor::new(text.clone()).map_err(|failed| format!("{text:?} is not an actor: {failed}"))
    }
    fn spelled(&self) -> &str {
        self.as_str()
    }
}

// The file names the logins whois answers with; nothing else mints one.
impl Text for Login {
    fn read(text: String) -> Result<Self, String> {
        Ok(Login::from_whois(text))
    }
    fn spelled(&self) -> &str {
        self.as_str()
    }
}

impl Text for PublicKey {
    fn read(text: String) -> Result<Self, String> {
        PublicKey::new(text.clone()).map_err(|failed| format!("{text:?} is not a key: {failed}"))
    }
    fn spelled(&self) -> &str {
        self.as_str()
    }
}

/// One [`Text`] value.
mod text {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    use super::Text;

    pub(super) fn serialize<T: Text, S: Serializer>(value: &T, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(value.spelled())
    }

    pub(super) fn deserialize<'de, T: Text, D: Deserializer<'de>>(input: D) -> Result<T, D::Error> {
        T::read(String::deserialize(input)?).map_err(D::Error::custom)
    }
}

/// A list of [`Text`] values.
mod texts {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    use super::Text;

    pub(super) fn serialize<T: Text, S: Serializer>(
        values: &[T],
        out: S,
    ) -> Result<S::Ok, S::Error> {
        out.collect_seq(values.iter().map(Text::spelled))
    }

    pub(super) fn deserialize<'de, T: Text, D: Deserializer<'de>>(
        input: D,
    ) -> Result<Vec<T>, D::Error> {
        Vec::<String>::deserialize(input)?
            .into_iter()
            .map(|text| T::read(text).map_err(D::Error::custom))
            .collect()
    }
}

/// A table of [`Text`] keys to [`Text`] values.
mod text_map {
    use std::collections::BTreeMap;

    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    use super::Text;

    pub(super) fn serialize<K: Text, V: Text, S: Serializer>(
        values: &BTreeMap<K, V>,
        out: S,
    ) -> Result<S::Ok, S::Error> {
        out.collect_map(
            values
                .iter()
                .map(|(key, value)| (key.spelled(), value.spelled())),
        )
    }

    pub(super) fn deserialize<'de, K: Text + Ord, V: Text, D: Deserializer<'de>>(
        input: D,
    ) -> Result<BTreeMap<K, V>, D::Error> {
        BTreeMap::<String, String>::deserialize(input)?
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    K::read(key).map_err(D::Error::custom)?,
                    V::read(value).map_err(D::Error::custom)?,
                ))
            })
            .collect()
    }
}

/// `[mail.junk]`: the domains counted in a digest and never listed (`churn`),
/// and those never churned however a seed lists them (`keep`).
mod junk {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use suzatary_core::mail::junk::{ChurnList, KeepList};

    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct Spelled {
        #[serde(default)]
        churn: Vec<String>,
        #[serde(default)]
        keep: Vec<String>,
    }

    pub(super) fn is_empty(list: &ChurnList) -> bool {
        list == &ChurnList::default()
    }

    pub(super) fn serialize<S: Serializer>(list: &ChurnList, out: S) -> Result<S::Ok, S::Error> {
        Spelled {
            churn: list
                .iter()
                .map(|domain| domain.as_str().to_owned())
                .collect(),
            keep: list
                .keep()
                .iter()
                .map(|domain| domain.as_str().to_owned())
                .collect(),
        }
        .serialize(out)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<ChurnList, D::Error> {
        let spelled = Spelled::deserialize(input)?;
        let keep =
            KeepList::new(spelled.keep.iter().map(String::as_str)).map_err(D::Error::custom)?;
        ChurnList::new(spelled.churn.iter().map(String::as_str), keep).map_err(D::Error::custom)
    }
}

fn is_none_declared(repos: &Declared) -> bool {
    repos.iter().next().is_none()
}

/// `[[secretary.repos]]`: each repository by name, with its check.
mod repos {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use suzatary_core::repos::{Declared, RepoName, RepoTarget};

    #[derive(Deserialize, Serialize)]
    struct Entry {
        name: RepoName,
        #[serde(flatten)]
        target: RepoTarget,
    }

    pub(super) fn serialize<S: Serializer>(repos: &Declared, out: S) -> Result<S::Ok, S::Error> {
        out.collect_seq(repos.iter().map(|(name, target)| Entry {
            name: name.clone(),
            target: target.clone(),
        }))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<Declared, D::Error> {
        Declared::new(
            Vec::<Entry>::deserialize(input)?
                .into_iter()
                .map(|entry| (entry.name, entry.target)),
        )
        .map_err(D::Error::custom)
    }
}
