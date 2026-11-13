//! `instance.toml` as written: each value parsed to its type, each path unrooted until
//! [`super::Instance::read`]. `init` and the e2e fixture write the file through these types.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use prodrome_core::event::Actor;
use prodrome_core::sign::PublicKey;
use serde::{Deserialize, Serialize};
use suzatary_base::capability::Login;
use suzatary_base::chains::ChainName;
use suzatary_base::drive_path::DrivePath;
use suzatary_base::persona::Persona;
use suzatary_core::connect::Coding;
use suzatary_core::mail::account::Roster;
use suzatary_core::mail::junk::ChurnList;
use suzatary_core::model_name::ModelName;
use suzatary_core::plugin::Manifest;
use suzatary_core::repos::Declared;
use suzatary_core::web::AllowRule;

use super::{CodeRoot, DataRoot, Reserve};

/// The whole file. A key the binary does not read refuses the file.
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
    /// The intent to notify, declared, not inferred from which secret resolves; absent is never.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) notify: Option<Notify>,
    /// Absent, a tap on a coding session is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) coding: Option<Coding>,
    /// The cache tier's root under the data root (`cache` when absent): box-local, never pushed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cache: Option<PathBuf>,
    /// Every plugin this box runs. Never serialised: a tap writes its table through `toml_edit`
    /// (`connections.rs`).
    #[serde(default, skip_serializing)]
    pub(crate) plugins: Vec<Manifest>,
    /// The deployment's root (SPEC law 42), required; `suzatary init` writes it.
    pub(crate) signatures: Signatures,
}

/// `[signatures]`: the public root key. Its seed, `SUZATARY_BOX_KEY` in secretspec, is never
/// written here.
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
    /// The file at `path`, each value parsed to its type.
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

/// One `[[chains]]` entry; order matters: the first is the default chain.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Chain {
    pub(crate) name: ChainName,
    pub(crate) events: PathBuf,
    #[serde(with = "texts")]
    pub(crate) untrusted: Vec<Actor>,
    #[serde(with = "texts")]
    pub(crate) actors: Vec<Actor>,
    /// Whom a command with no `--actor` acts as; must be trusted and on the roster.
    #[serde(with = "text")]
    pub(crate) owner: Actor,
}

// `toml` serialises plain values ahead of tables, so `act_as` and `serving` come last.
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
    pub host: String,
    pub port: u16,
    /// Where `tailscale cert` puts `cert.pem` and `key.pem`.
    pub tls_dir: PathBuf,
    /// The frontend build, the one directory served as files.
    pub dist: PathBuf,
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
    pub root: PathBuf,
    /// The triage state file, the sole dedup authority.
    pub state: PathBuf,
    pub notmuch: PathBuf,
    /// Absent means no mail index: a refusal, never an ambient fallback.
    #[serde(default)]
    pub notmuch_config: Option<PathBuf>,
    /// The mbsync configuration the Proton channel is pinned to.
    pub mbsyncrc: PathBuf,
    /// A bare timestamp of the last triage pass that pushed, kept off the chain because the
    /// chain goes silent exactly when a pass cannot commit.
    pub heartbeat: PathBuf,
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
    /// Bytes its `context` may spend on the secretary's memory beyond the manual's cap (at most
    /// this many tokens, a token being at least one byte).
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

/// `[secretary]`, defaulted key by key, so the secretary's routes exist on every deployment.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Secretary {
    /// The secretary's directory: the memory and the files a conversation holds.
    pub conversations: PathBuf,
    /// The cache of rendered answer blocks, beside `conversations`, never inside it.
    pub renders: PathBuf,
    /// The inbox's store: every proposal and what became of it.
    pub inbox: PathBuf,
    /// The memory's history by id; the words are [`Secretary::memory_texts`].
    pub memory: PathBuf,
    /// The digest's history by node name; the words are [`Secretary::digest_texts`].
    pub digests: PathBuf,
    /// The two topic stores, box-local, vaulted and never pushed.
    pub topics: Topics,
    /// The model for the conversation, reviews and titles, pinned so it cannot drift.
    pub model: ModelName,
    /// Whether the compactor runs; the five keys after it are read only when it does.
    pub compact: bool,
    /// The compactor's models, in the order a span is owed to them: a span one model cannot say
    /// in a line goes to the next.
    pub compactors: suzatary_core::nonempty::NonEmpty<ModelName>,
    /// The share of the subscription the compactor never takes.
    pub reserve: Reserve,
    pub compact_calls_per_hour: u32,
    /// The most bytes it sends in any twenty-four hours.
    pub compact_bytes_per_day: u64,
    /// How many of its steps may run at once, each on a node no other step holds.
    pub compact_workers: std::num::NonZeroUsize,
    /// The settings file that fences the model's subprocess, under the code root; the shipped
    /// deny-all unless the file names another.
    pub settings: PathBuf,
    /// The hosts it may read without asking; "Always allow" appends here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub web_allow: Vec<AllowRule>,
    /// The Drive folders it reads without asking; "Always allow this folder" appends here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub drive_allow: Vec<DrivePath>,
    /// The Proton Drive CLI; `$HOME/bin/proton-drive` when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive_cli: Option<PathBuf>,
    /// The repositories the secretary may reach, each with its check; any other is refused.
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
            // Off by default: the first run compacts every line already stored.
            compact: false,
            compactors: suzatary_core::nonempty::NonEmpty::new(
                ModelName::parse("claude-sonnet-5-5").expect("a model name"),
                vec![ModelName::parse("claude-opus-5-5").expect("a model name")],
            ),
            reserve: Reserve::default(),
            // An import of every past line takes days, never an afternoon of the subscription.
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
    /// The compactor as these keys set it: off, or its models within a budget.
    #[must_use]
    pub fn compacting(&self) -> suzatary_core::secretary::compacting::Compacting {
        use suzatary_core::secretary::compacting::Compacting;
        if !self.compact {
            return Compacting::Off;
        }
        Compacting::On {
            ladder: self.compactors.clone(),
            budget: suzatary_core::secretary::compact_budget::Budget {
                calls_per_hour: self.compact_calls_per_hour,
                bytes_per_day: self.compact_bytes_per_day,
            },
            workers: self.compact_workers,
        }
    }

    /// The conversations' lines, pushed with their words locked.
    #[must_use]
    pub fn line_store(&self) -> PathBuf {
        self.beside("-lines")
    }

    /// The keys of the lines' words: box-local, vaulted and never pushed.
    #[must_use]
    pub fn line_keys(&self) -> PathBuf {
        self.beside("-lines-keys")
    }

    /// The conversations; an erasure also empties `renders` and `sessions`.
    #[must_use]
    pub fn conversation_stores(
        &self,
        renders: std::sync::Arc<dyn suzatary_core::render_cache::RenderCache + Send + Sync>,
        sessions: std::sync::Arc<
            dyn suzatary_core::secretary::model_sessions::ModelSessions + Send + Sync,
        >,
    ) -> suzatary_io::conversations::ConversationStores {
        suzatary_io::conversations::ConversationStores::open(
            self.line_store(),
            self.line_keys(),
            renders,
            sessions,
        )
    }

    /// The memory's texts, box-local and never pushed.
    #[must_use]
    pub fn memory_texts(&self) -> PathBuf {
        self.beside("-memory-texts")
    }

    /// The digest's summaries' words, box-local and never pushed.
    #[must_use]
    pub fn digest_texts(&self) -> PathBuf {
        self.beside("-digest-texts")
    }

    /// The owner's uploads, box-local and git-ignored.
    #[must_use]
    pub fn uploads(&self) -> PathBuf {
        self.conversations.join("attachments")
    }

    /// The files verbs hand the model, box-local and git-ignored.
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

/// `[secretary] topics`. A topic name is words, so both stores stay on the box: the vault
/// carries them and the data push refuses them.
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
    #[must_use]
    pub fn open(&self) -> suzatary_io::topics::Topics {
        suzatary_io::topics::Topics::open(self.memberships.clone(), self.aliases.clone())
    }
}

/// `[notify]`: a non-empty `channels` means push secrets are resolved at startup.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Notify {
    #[serde(default)]
    pub(crate) channels: Vec<suzatary_core::connect::Channel>,
    /// The Firebase project's number the box hands the Android portal at pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fcm_sender: Option<suzatary_api::portal::Sender>,
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
    use suzatary_base::repo_ids::RepoName;
    use suzatary_core::repos::{Declared, RepoTarget};

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
