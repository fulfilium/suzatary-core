//! notmuch and the maildir, read only, as [`Mailbox`].
//!
//! Every list asks the index for the newest ids first and only loads those:
//! loading hashes every file a query matches, and notmuch has no LIMIT. The
//! parsing ([`dedup`], [`text`]) is pure over captured output.

pub mod attachments;
pub mod dedup;
pub mod forwarded;
pub mod held;
pub mod message;
pub mod text;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::present::Present as _;
use crate::run::{Cmd, succeed};

/// What notmuch reads of its environment beyond [`crate::run::BASE`].
const NOTMUCH: &[&str] = &["NOTMUCH_*"];

/// `cmd` told which index to use: the `config` named, and no other `NOTMUCH_*` variable, since
/// `NOTMUCH_DATABASE` or `NOTMUCH_PROFILE` from the environment would point it at another one;
/// with none named, the environment's.
#[must_use]
pub fn indexed_by(cmd: Cmd, config: Option<&Path>) -> Cmd {
    match config {
        Some(config) => cmd.env("NOTMUCH_CONFIG", config),
        None => cmd.pass(NOTMUCH),
    }
}
use self::message::NotmuchMessage;
use suzatary_core::capability::{Datetime, MailError, Mailbox, Query};
use suzatary_core::enclosed::Enclosure;
use suzatary_core::file_read::FileRead;
use suzatary_core::held_value::{self, Handle, Reopened, Reveal, Revealed};
use suzatary_core::mail::account::{MailAccount, Roster};
use suzatary_core::mail::known::Known;
use suzatary_core::mail::local_state::{Ambiguous, Kept};
use suzatary_core::mail::verbs;
use suzatary_core::mail::{
    AttachmentFile, AttachmentMeta, MAX_BODY_CHARS, MailLink, MailMessage, MailRow, Stored, clip,
    headers_of, web_links,
};

/// notmuch as a subprocess, and the maildir it names, as [`Mailbox`].
pub struct Notmuch {
    binary: PathBuf,
    /// The root each account's `subdir` is relative to.
    mail_root: PathBuf,
    /// The triage's bookkeeping, read for badges and hash prefixes.
    ledger: crate::state::Ledger,
    /// Every account there is.
    accounts: Roster,
    /// Never inherited, so a fixture cannot read the box's own index.
    config: Option<PathBuf>,
    pdftotext: crate::pdftotext::Pdftotext,
}

impl std::fmt::Debug for Notmuch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notmuch")
            .field("binary", &self.binary)
            .field("mail_root", &self.mail_root)
            .field("ledger", &self.ledger)
            .field("accounts", &self.accounts.accounts().len())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Notmuch {
    /// The maildir under `mail_root`, the triage's bookkeeping in `ledger`, and the accounts
    /// there are.
    #[must_use]
    pub fn new(
        mail_root: impl Into<PathBuf>,
        ledger: crate::state::Ledger,
        accounts: Roster,
    ) -> Self {
        Self {
            binary: PathBuf::from("notmuch"),
            mail_root: mail_root.into(),
            ledger,
            accounts,
            config: None,
            pdftotext: crate::pdftotext::Pdftotext::new("."),
        }
    }

    /// A different `notmuch` binary.
    #[must_use]
    pub fn with_binary(mut self, notmuch: impl Into<PathBuf>) -> Self {
        self.binary = notmuch.into();
        self
    }

    /// `pdftotext`'s nix fallback.
    #[must_use]
    pub fn with_pdftotext_repo(mut self, repo: impl Into<PathBuf>) -> Self {
        self.pdftotext = crate::pdftotext::Pdftotext::new(repo);
        self
    }

    /// The notmuch config every call carries as `NOTMUCH_CONFIG`.
    #[must_use]
    pub fn with_config(mut self, config: impl Into<PathBuf>) -> Self {
        self.config = Some(config.into());
        self
    }

    /// The invocation, or the refusal when no config is named, spawning nothing.
    fn command(&self, args: &[&str]) -> Result<Cmd, MailError> {
        let Some(config) = &self.config else {
            return Err(MailError::Unreachable(
                "this deployment has no mail index".to_owned(),
            ));
        };
        Ok(indexed_by(Cmd::new(&self.binary).args(args), Some(config)))
    }

    /// notmuch's answer as it wrote it: it copies a message's bytes into its JSON unchanged, so
    /// the answer need not be UTF-8, and each reader decodes it (`message::Text`).
    fn run(&self, args: &[&str]) -> Result<Vec<u8>, MailError> {
        Ok(succeed(&self.command(args)?)
            .map_err(|e| MailError::Unreachable(e.to_string()))?
            .stdout)
    }

    /// The `limit` newest message ids matching `query`, from the index alone.
    fn newest_ids(&self, query: &str, limit: usize) -> Result<Vec<String>, MailError> {
        let limit_flag = format!("--limit={limit}");
        let raw = self.run(&[
            "search",
            "--format=json",
            "--output=messages",
            "--sort=newest-first",
            &limit_flag,
            query,
        ])?;
        parse_id_array(&raw)
    }

    /// Every thread `query` touches.
    fn thread_ids(&self, query: &str) -> Result<Vec<String>, MailError> {
        let raw = self.run(&["search", "--format=json", "--output=threads", query])?;
        parse_id_array(&raw)
    }

    /// Every message `query` matches, folded by content hash; it reads and hashes each file, so
    /// page reads bound `query` through [`Self::newest_ids`] first.
    fn load_messages(&self, query: &str) -> Result<BTreeMap<String, CanonicalMessage>, MailError> {
        let raw = self.run(&["show", "--format=json", "--body=false", query])?;
        let mut messages: BTreeMap<String, CanonicalMessage> = BTreeMap::new();
        for msg in message::matched(&raw)? {
            // A file that vanished since notmuch named it (a concurrent sync) costs one row.
            let Some(bytes) = std::fs::read(&msg.file).present().map_err(|failed| {
                MailError::Unreachable(format!("reading {}: {failed}", msg.file.display()))
            })?
            else {
                continue;
            };
            let hash = dedup::canonical_hash(&bytes);
            let values = held::values(&bytes, &[]);
            let parsed = mail_parser::MessageParser::default().parse(&bytes);
            let sender = parsed
                .as_ref()
                .and_then(mail_parser::Message::from)
                .and_then(|a| a.iter().next())
                .and_then(|a| a.address.as_ref())
                .map(|s| s.trim().to_lowercase())
                .unwrap_or_default();
            let title = parsed
                .as_ref()
                .and_then(mail_parser::Message::subject)
                .unwrap_or_default()
                .to_owned();
            let timestamp = parsed
                .as_ref()
                .and_then(|m| m.header_raw(mail_parser::HeaderName::Date))
                .and_then(dedup::parse_rfc2822_date);
            let (sender, title) = (
                held_value::mask(&sender, &msg.id, &values),
                held_value::mask(&title, &msg.id, &values),
            );
            let source = Source {
                account: self.account_for(&msg.file),
                container: container_of(&msg.tags),
                native_id: msg.id,
                path: msg.file,
            };
            messages
                .entry(hash.clone())
                .and_modify(|m| m.add_source(source.clone()))
                .or_insert_with(|| CanonicalMessage {
                    hash,
                    sender,
                    title,
                    timestamp,
                    primary: source,
                    also: Vec::new(),
                });
        }
        Ok(messages)
    }

    /// Every full message `query` matches; `--include-html` is required because without it
    /// notmuch omits an html part's content and an html-only message arrives empty.
    fn show_query(&self, query: &str) -> Result<Vec<Shown>, MailError> {
        let raw = self.run(&["show", "--format=json", "--include-html", query])?;
        Ok(message::matched(&raw)?.into_iter().map(shown_of).collect())
    }

    /// One message by notmuch id, unquoted because the id is notmuch's own.
    fn show_by_native_id(&self, id: &str) -> Result<Option<Shown>, MailError> {
        Ok(self.show_query(&format!("id:{id}"))?.into_iter().next())
    }

    /// A canonical-hash prefix's notmuch id, from the locators `state.json` recorded.
    fn native_id_for_hash(&self, prefix: &str) -> Result<Option<String>, MailError> {
        let state = self.load_state()?;
        Ok(resolve_locator(&state, prefix)?.id())
    }

    /// Resolve `id` as a notmuch id, then as a hash prefix through the recorded locators.
    ///
    /// Never [`Self::scan_for_hash`]: its eight-second rescan must not sit behind a URL, so an
    /// unrecorded hash is a 404.
    fn resolve_show(&self, id: &str) -> Result<Option<Shown>, MailError> {
        if let Some(shown) = self.show_by_native_id(id)? {
            return Ok(Some(shown));
        }
        if !is_hex_prefix(id) {
            return Ok(None);
        }
        match self.native_id_for_hash(id)? {
            Some(native) => self.show_by_native_id(&native),
            None => Ok(None),
        }
    }

    /// The triage's bookkeeping, read on every call because other processes write it.
    fn load_state(&self) -> Result<Known, MailError> {
        self.ledger.read().map_err(MailError::Unreachable)
    }

    /// The account whose maildir `path` lives under, or `""` (the one spelling of absence).
    fn account_for(&self, path: &Path) -> String {
        let Ok(relative) = path.strip_prefix(&self.mail_root) else {
            return String::new();
        };
        let Some(std::path::Component::Normal(subdir)) = relative.components().next() else {
            return String::new();
        };
        let subdir = subdir.to_string_lossy();
        self.accounts
            .accounts()
            .iter()
            .find(|a| a.subdir == subdir)
            .map(|a| a.email.to_string())
            .unwrap_or_default()
    }

    fn account_named(&self, address: Option<&str>) -> Result<Option<&MailAccount>, MailError> {
        match address {
            None => Ok(None),
            Some(addr) => self
                .accounts
                .named(addr)
                .map(Some)
                .ok_or_else(|| MailError::Absent {
                    kind: "account",
                    name: addr.to_owned(),
                }),
        }
    }

    /// Newest ids from the index, then only those canonicalised.
    fn recent(
        &self,
        query: &str,
        limit: usize,
        before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        let bounded = before_query(query, before);
        let ids = self.newest_ids(&bounded, limit)?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        self.rows(&by_id_query(refs), limit)
    }

    /// Any notmuch query as app rows, newest first.
    fn rows(&self, query: &str, limit: usize) -> Result<Vec<MailRow>, MailError> {
        let messages = self.load_messages(query)?;
        let state = self.load_state()?;
        let mut ordered: Vec<&CanonicalMessage> = messages.values().collect();
        ordered.sort_by_key(|m| std::cmp::Reverse(epoch_of(m)));
        ordered.truncate(limit);
        Ok(ordered.into_iter().map(|m| row_of(m, &state)).collect())
    }
}

impl Notmuch {
    /// One canonical message as the agent reads a row.
    fn summary_of(message: &CanonicalMessage) -> verbs::Summary {
        let primary = &message.primary;
        verbs::Summary {
            hash: message.hash.clone(),
            id: primary.native_id.clone(),
            sender: message.sender.clone(),
            title: message.title.clone(),
            timestamp: message.timestamp.map(|parsed| {
                verbs::shown_timestamp(
                    parsed.year,
                    parsed.month,
                    parsed.day,
                    parsed.hour,
                    parsed.minute,
                    parsed.second,
                    parsed.tz_offset_minutes,
                )
            }),
            container: primary.container.clone(),
            sources: message.sources(),
        }
    }

    /// Newest first, capped.
    fn newest_first(
        messages: BTreeMap<String, CanonicalMessage>,
        limit: Option<usize>,
    ) -> Vec<verbs::Summary> {
        let mut ordered: Vec<CanonicalMessage> = messages.into_values().collect();
        ordered.sort_by_key(|m| std::cmp::Reverse(epoch_of(m)));
        if let Some(cap) = limit {
            ordered.truncate(cap);
        }
        ordered.iter().map(Self::summary_of).collect()
    }

    /// Any notmuch query as the agent's summaries, newest first.
    ///
    /// Unlike [`Mailbox::search`] it loads every match and then truncates, so `limit` cuts a set
    /// the agent can reason about rather than a window the index handed back.
    pub fn summaries(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<verbs::Summary>, MailError> {
        Ok(Self::newest_first(self.load_messages(query)?, limit))
    }

    /// The triage queue: inbox mail whose hash `state.json` has not seen.
    ///
    /// Gated on the hash, not the unread flag, so mail read on a phone is still triaged; the
    /// 14-day floor only bounds a re-tag flood, since the fetch window is seven days.
    pub fn queue(
        &self,
        state: &Known,
        limit: Option<usize>,
    ) -> Result<Vec<verbs::Summary>, MailError> {
        let mut all: BTreeMap<String, CanonicalMessage> = BTreeMap::new();
        for account in self.accounts.accounts() {
            let query = format!("tag:inbox and date:14d.. and {}", account.query());
            all.extend(self.load_messages(&query)?);
        }
        let fresh: BTreeMap<String, CanonicalMessage> = all
            .into_iter()
            .filter(|(hash, message)| {
                state.decided(hash).is_none() && !state.is_noise(&message.sender)
            })
            .collect();
        Ok(Self::newest_first(fresh, limit))
    }

    /// One message in full, by notmuch id or by canonical hash (bridged through `state.json`'s
    /// recorded locators, since notmuch indexes only `Message-ID`s).
    pub fn full_message(&self, id: &str, state: &Known) -> Result<verbs::Show, MailError> {
        let shown = match self.show_by_native_id(id)? {
            Some(shown) => Some(shown),
            None if is_hex_prefix(id) => match state.local.resolve(id) {
                Err(ambiguous) => return Ok(verbs::Show::Ambiguous(ambiguous)),
                Ok(Some((
                    _,
                    Kept {
                        native_id: Some(native),
                        ..
                    },
                ))) => self.show_by_native_id(native)?,
                Ok(
                    Some((
                        _,
                        Kept {
                            native_id: None, ..
                        },
                    ))
                    | None,
                ) => None,
            },
            None => None,
        };
        Ok(shown.map_or_else(
            || verbs::Show::NotFound { id: id.to_owned() },
            |shown| verbs::Show::Shown(Self::message_of(shown)),
        ))
    }

    /// A shown message as the agent reads it.
    fn message_of(shown: Shown) -> verbs::Message {
        verbs::Message {
            id: shown.id,
            tags: shown.tags,
            headers: shown.headers.into_iter().collect(),
            body: shown.body,
            links: shown
                .links
                .into_iter()
                .map(|link| verbs::Link {
                    url: link.url,
                    label: link.label,
                })
                .collect(),
        }
    }

    /// The raw maildir file behind `id`, since notmuch's JSON never carries a binary part.
    fn raw_bytes(&self, id: &str) -> Result<Vec<u8>, MailError> {
        let absent = || MailError::Absent {
            kind: "message",
            name: id.to_owned(),
        };
        let Some(shown) = self.resolve_show(id)? else {
            return Err(absent());
        };
        let query = by_id_query(std::iter::once(shown.id.as_str()));
        let messages = self.load_messages(&query)?;
        let message = messages.values().next().ok_or_else(absent)?;
        std::fs::read(&message.primary.path)
            .map_err(|failed| MailError::Unreachable(failed.to_string()))
    }

    /// The file notmuch holds for the message `id`, its first when it holds several, and that
    /// file's size; `None` when it holds none.
    fn file_of(&self, id: &str) -> Result<Option<(PathBuf, u64)>, MailError> {
        let query = by_id_query(std::iter::once(id));
        if query.is_empty() {
            return Ok(None);
        }
        let listed = self.run(&["search", "--output=files", "--duplicate=1", &query])?;
        let listed = String::from_utf8_lossy(&listed);
        let Some(path) = listed.lines().map(str::trim).find(|line| !line.is_empty()) else {
            return Ok(None);
        };
        let path = PathBuf::from(path);
        let size = std::fs::metadata(&path)
            .map_err(|failed| MailError::Unreachable(failed.to_string()))?
            .len();
        Ok(Some((path, size)))
    }

    /// At most `limit + 1` bytes of the file at `path`, so a file that grew since it was
    /// measured reads as longer than `limit` and no further.
    fn read_file(path: &Path, limit: u64) -> Result<Vec<u8>, MailError> {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(limit.saturating_add(1)).read_to_end(&mut bytes))
            .map_err(|failed| MailError::Unreachable(failed.to_string()))?;
        Ok(bytes)
    }

    /// A last-resort hash-prefix lookup by re-hashing thirty days of mail (about eight seconds),
    /// so only a hand-run write may call it.
    pub fn scan_for_hash(&self, prefix: &str) -> Result<Option<String>, MailError> {
        for (hash, message) in self.load_messages("date:30d..")? {
            if hash.starts_with(prefix) {
                return Ok(Some(message.primary.native_id));
            }
        }
        Ok(None)
    }

    /// Each file notmuch holds for the message `id` whose bytes have the canonical hash `hash`,
    /// with its path under the mail root; a file outside the mail root is left out, since no
    /// account holds it.
    ///
    /// # Errors
    /// notmuch cannot be asked, or a file it names cannot be read.
    pub fn files_holding(
        &self,
        id: &str,
        hash: &str,
    ) -> Result<Vec<(PathBuf, Vec<u8>)>, MailError> {
        let query = by_id_query(std::iter::once(id));
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let listed = self.run(&["search", "--output=files", &query])?;
        let mut files = Vec::new();
        for line in String::from_utf8_lossy(&listed).lines().map(str::trim) {
            let path = Path::new(line);
            let Ok(relative) = path.strip_prefix(&self.mail_root) else {
                continue;
            };
            let Some(bytes) = std::fs::read(path).present().map_err(|failed| {
                MailError::Unreachable(format!("reading {}: {failed}", path.display()))
            })?
            else {
                continue;
            };
            if dedup::canonical_hash(&bytes) == hash {
                files.push((relative.to_path_buf(), bytes));
            }
        }
        Ok(files)
    }
}

/// What a hash prefix resolved to in the dedup authority.
enum Locator {
    /// No marked message begins with this prefix.
    Unmarked,
    /// One does, marked before locators were recorded, so only a rescan finds it.
    Unrecorded,
    /// One does, and this is its notmuch id.
    Known(String),
}

impl Locator {
    fn id(self) -> Option<String> {
        match self {
            Locator::Known(id) => Some(id),
            Locator::Unmarked | Locator::Unrecorded => None,
        }
    }
}

/// The locator `state.json` recorded for a hash prefix; an ambiguous prefix is a query error.
fn resolve_locator(state: &Known, prefix: &str) -> Result<Locator, MailError> {
    match state.local.resolve(prefix) {
        Ok(None) => Ok(Locator::Unmarked),
        Ok(Some((_, entry))) => Ok(entry
            .native_id
            .clone()
            .map_or(Locator::Unrecorded, Locator::Known)),
        Err(Ambiguous { prefix, matches }) => {
            Err(MailError::Query(Ambiguous { prefix, matches }.to_string()))
        }
    }
}

/// A row plus what a page needs: thread, whitelisted headers, clipped body, filtered links.
fn full(
    message: &CanonicalMessage,
    shown: Option<&Shown>,
    state: &Known,
    thread: String,
) -> MailMessage {
    let raw_body = shown.map_or("", |s| s.body.as_str());
    let (body, truncated) = clip(raw_body, MAX_BODY_CHARS);
    let headers = shown
        .map(|s| {
            let pairs: Vec<(String, String)> = s
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            headers_of(&pairs)
        })
        .unwrap_or_default();
    let links = shown
        .map(|s| web_links(s.links.clone()))
        .unwrap_or_default();
    MailMessage {
        row: row_of(message, state),
        thread,
        headers,
        body,
        truncated,
        links,
    }
}

/// One content hash and every place it was seen: the first appearance read, whose locator and
/// folder its row shows, and the others.
struct CanonicalMessage {
    hash: String,
    sender: String,
    title: String,
    timestamp: Option<dedup::ParsedDate>,
    primary: Source,
    also: Vec<Source>,
}

impl CanonicalMessage {
    fn add_source(&mut self, source: Source) {
        if self.primary != source && !self.also.contains(&source) {
            self.also.push(source);
        }
    }

    /// How many places it was seen.
    fn sources(&self) -> usize {
        1 + self.also.len()
    }
}

/// One appearance of a canonical message.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    account: String,
    container: String,
    native_id: String,
    path: PathBuf,
}

/// Newest-first sort key: epoch seconds of the `Date:` header, or `0` when undated.
fn epoch_of(message: &CanonicalMessage) -> i64 {
    message
        .timestamp
        .and_then(|p| p.epoch_seconds())
        .unwrap_or(0)
}

/// One [`MailRow`]: the message beside what `state.json` recorded about its hash.
fn row_of(message: &CanonicalMessage, state: &Known) -> MailRow {
    let seen = state.decided(&message.hash);
    let primary = &message.primary;
    MailRow {
        hash: message.hash.clone(),
        id: primary.native_id.clone(),
        sender: message.sender.clone(),
        subject: message.title.clone(),
        date: message.timestamp.and_then(|p| p.to_naive_datetime()),
        container: primary.container.clone(),
        account: primary.account.clone(),
        disposition: suzatary_core::mail::Disposed::from(seen),
    }
}

/// notmuch tags that describe state, not a folder.
const STATE_TAGS: [&str; 10] = [
    "new",
    "unread",
    "attachment",
    "signed",
    "encrypted",
    "flagged",
    "replied",
    "sent",
    "draft",
    "inbox",
];

/// A message's home folder: `"inbox"` if tagged, else its first non-state tag, else `"inbox"`.
fn container_of(tags: &[String]) -> String {
    if tags.iter().any(|t| t == "inbox") {
        return "inbox".to_owned();
    }
    tags.iter()
        .find(|t| !STATE_TAGS.contains(&t.as_str()))
        .cloned()
        .unwrap_or_else(|| "inbox".to_owned())
}

/// A canonical-hash prefix, `[0-9a-f]{6,64}`; a mixed-case id is a notmuch id.
fn is_hex_prefix(id: &str) -> bool {
    (6..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}

/// `notmuch search`'s flat array of ids; any other shape is refused.
fn parse_id_array(raw: &[u8]) -> Result<Vec<String>, MailError> {
    let text = message::Text::of(raw);
    let ids: Vec<String> = serde_json::from_str(text.as_str()).map_err(|e| {
        MailError::Unreachable(format!("notmuch's id list was not an array of ids: {e}"))
    })?;
    match ids.iter().find(|id| text.lost(id)) {
        Some(id) => Err(message::unnamed(id)),
        None => Ok(ids),
    }
}

/// One `notmuch show` message, shared by the message and thread paths so a thread costs one
/// call.
struct Shown {
    id: String,
    /// Printed only in the agent's `show` answer.
    tags: Vec<String>,
    headers: HashMap<String, String>,
    body: String,
    links: Vec<MailLink>,
}

/// A shown message as a model may read it: every identity number a placeholder (`held_value`).
fn shown_of(msg: NotmuchMessage) -> Shown {
    let (shown, values) = unmasked(msg);
    let mask = |text: &str| held_value::mask(text, &shown.id, &values);
    Shown {
        headers: shown
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), mask(value)))
            .collect(),
        body: mask(&shown.body),
        links: shown
            .links
            .iter()
            .map(|link| MailLink {
                url: mask(&link.url),
                label: mask(&link.label),
            })
            .collect(),
        id: shown.id,
        tags: shown.tags,
    }
}

/// A shown message as notmuch shows it, and its held values: its file's, then the header values
/// by name and the body, so a reveal counts them as the mask did. A file that cannot be read
/// gives the shown texts' values alone, which a reveal then finds gone.
fn unmasked(msg: NotmuchMessage) -> (Shown, held_value::Values) {
    let raw = std::fs::read(&msg.file).unwrap_or_default();
    let body = msg.body.unwrap_or_default();
    let shown = Shown {
        id: msg.id,
        tags: msg.tags,
        headers: msg.headers,
        body: text::body_text(&body),
        links: text::extract_links(&body),
    };
    let mut named: Vec<(&String, &String)> = shown.headers.iter().collect();
    named.sort();
    let texts: Vec<&str> = named
        .into_iter()
        .map(|(_, value)| value.as_str())
        .chain(std::iter::once(shown.body.as_str()))
        .collect();
    let values = held::values(&raw, &texts);
    (shown, values)
}

/// A notmuch query naming exactly these ids; an id with a quote or whitespace is dropped rather
/// than escaped, since notmuch's own ids never carry one.
pub(crate) fn by_id_query<'a>(ids: impl IntoIterator<Item = &'a str>) -> String {
    ids.into_iter()
        .filter(|id| !id.contains('"') && !id.chars().any(char::is_whitespace))
        .map(|id| format!("id:\"{id}\""))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// `query`, narrowed to one account's maildir.
fn scoped_query(query: &str, account: Option<&MailAccount>) -> String {
    match account {
        None => query.to_owned(),
        Some(a) => format!("({query}) and ({})", a.query()),
    }
}

/// The inbox as notmuch spells it.
fn inbox_query(account: Option<&MailAccount>) -> String {
    scoped_query("tag:inbox", account)
}

/// `query`, bounded to strictly before the page cursor, so a page never repeats a row.
///
/// The cursor is read as UTC because the wire type has no zone, which can skip or repeat one row
/// at a page boundary.
fn before_query(query: &str, before: Option<Datetime>) -> String {
    match before {
        None => query.to_owned(),
        Some(dt) => {
            let date = chrono::NaiveDate::from_ymd_opt(dt.year(), dt.month(), dt.day())
                .expect("a Datetime's fields are already validated");
            let naive = date
                .and_hms_opt(dt.hour(), dt.minute(), dt.second())
                .expect("a Datetime's fields are already validated");
            let epoch = naive.and_utc().timestamp();
            format!("({query}) and date:..@{}", epoch - 1)
        }
    }
}

impl Reveal for Notmuch {
    fn reveal(&self, handle: &Handle) -> Result<Revealed, MailError> {
        let query = by_id_query(std::iter::once(handle.message()));
        if query.is_empty() {
            return Ok(Revealed::Gone);
        }
        let raw = self.run(&["show", "--format=json", "--include-html", &query])?;
        let Some(msg) = message::matched(&raw)?.into_iter().next() else {
            return Ok(Revealed::Gone);
        };
        let (_, values) = unmasked(msg);
        Ok(values
            .get(handle.ordinal())
            .map_or(Revealed::Gone, |(kind, value)| {
                Revealed::Value(held_value::Shown::new(kind, value))
            }))
    }

    fn original(&self, message: &str, size: u64) -> Result<Reopened, MailError> {
        Ok(match self.file_of(message)? {
            None => Reopened::Gone,
            Some((_, now)) if now != size => Reopened::Resized,
            Some((path, _)) => {
                let raw = Self::read_file(&path, size)?;
                if raw.len() as u64 == size {
                    Reopened::Read(forwarded::original(raw))
                } else {
                    Reopened::Resized
                }
            }
        })
    }
}

impl Mailbox for Notmuch {
    fn accounts(&self) -> Vec<String> {
        self.accounts
            .accounts()
            .iter()
            .map(|a| a.email.to_string())
            .collect()
    }

    fn inbox(
        &self,
        account: Option<&str>,
        limit: usize,
        before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        let account = self.account_named(account)?;
        self.recent(&inbox_query(account), limit, before)
    }

    fn search(
        &self,
        query: &Query,
        account: Option<&str>,
        limit: usize,
        before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        let account = self.account_named(account)?;
        self.recent(&scoped_query(query.as_str(), account), limit, before)
    }

    fn message(&self, id: &str) -> Result<MailMessage, MailError> {
        let absent = || MailError::Absent {
            kind: "message",
            name: id.to_owned(),
        };
        let Some(shown) = self.resolve_show(id)? else {
            return Err(absent());
        };
        let query = by_id_query(std::iter::once(shown.id.as_str()));
        let messages = self.load_messages(&query)?;
        let Some(message) = messages.values().next() else {
            return Err(absent());
        };
        let thread = self.thread_ids(&query)?.into_iter().next().ok_or_else(|| {
            MailError::Unreachable(format!("notmuch names no thread for {}", shown.id))
        })?;
        let state = self.load_state()?;
        Ok(full(message, Some(&shown), &state, thread))
    }

    fn thread(&self, thread: &str, limit: usize) -> Result<Vec<MailMessage>, MailError> {
        let query = format!("thread:{thread}");
        let messages = self.load_messages(&query)?;
        let mut ordered: Vec<&CanonicalMessage> = messages.values().collect();
        ordered.sort_by_key(|m| std::cmp::Reverse(epoch_of(m)));
        ordered.truncate(limit);

        let shown = self.show_query(&query)?;
        let by_native: HashMap<&str, &Shown> = shown.iter().map(|s| (s.id.as_str(), s)).collect();
        let state = self.load_state()?;

        // The newest `limit`, shown oldest first: the end of a thread is what is being read.
        Ok(ordered
            .into_iter()
            .rev()
            .map(|m| {
                let native = m.primary.native_id.as_str();
                full(m, by_native.get(native).copied(), &state, thread.to_owned())
            })
            .collect())
    }

    fn attachments(&self, id: &str) -> Result<Vec<AttachmentMeta>, MailError> {
        Ok(attachments::list(&self.raw_bytes(id)?))
    }

    fn stored(&self, id: &str, cap: u64) -> Result<Stored, MailError> {
        let (path, size) = self.file_of(id)?.ok_or_else(|| MailError::Absent {
            kind: "message",
            name: id.to_owned(),
        })?;
        if size > cap {
            return Ok(Stored::TooLarge { size });
        }
        let raw = Self::read_file(&path, cap)?;
        Ok(if raw.len() as u64 > cap {
            Stored::TooLarge {
                size: raw.len() as u64,
            }
        } else {
            Stored::Recorded(forwarded::stored(id, &raw))
        })
    }

    fn attachment(&self, id: &str, name: &str) -> Result<AttachmentFile, MailError> {
        let bytes = self.raw_bytes(id)?;
        let (mime, content) = attachments::find(&bytes, name).ok_or_else(|| MailError::Absent {
            kind: "attachment",
            name: name.to_owned(),
        })?;
        Ok(AttachmentFile {
            name: name.to_owned(),
            mime,
            content,
        })
    }

    fn read_attachment(&self, id: &str, name: &str) -> Result<FileRead, MailError> {
        let file = self.attachment(id, name)?;
        read_of(&file.mime, file.content, &self.pdftotext).map_err(MailError::Unreachable)
    }
}

/// A file read for the model: a PDF with its text layer through `pdftotext` beside it, anything
/// else as [`FileRead::of`] finds it; `Err` is a `pdftotext` failure, a fact about the box, not
/// the file.
pub(crate) fn read_of(
    mime: &str,
    content: Vec<u8>,
    pdftotext: &crate::pdftotext::Pdftotext,
) -> Result<FileRead, String> {
    let layer = match Enclosure::sniff(&content) {
        Some(Enclosure::Pdf) => Some(pdftotext.text(&content)?),
        Some(Enclosure::Image(_)) | None => None,
    };
    Ok(FileRead::of(mime, content, layer.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic numbers no person holds: SSN area 000, ITIN area 912, an EIN of zeros.
    const SSN: &str = "000-12-3456";
    const ITIN: &str = "912-70-1234";
    const EIN: &str = "00-0000001";

    /// A sent, an inbound and a clean message under `root`, indexed by the real notmuch under a
    /// config of their own, and the reader over them.
    fn indexed(root: &std::path::Path) -> (Notmuch, Known) {
        let mail = root.join("Mail");
        for leaf in [
            "ada/Sent/cur",
            "ada/Inbox/cur",
            "ada/Inbox/new",
            "ada/Inbox/tmp",
        ] {
            std::fs::create_dir_all(mail.join(leaf)).expect("a leaf");
        }
        let config = root.join("notmuch-config");
        std::fs::write(
            &config,
            format!(
                "[database]\npath={}\n[user]\nname=Ada\nprimary_email=ada@example.test\n\
             [new]\ntags=new\n",
                mail.display()
            ),
        )
        .expect("a notmuch config");
        let sent = format!(
            "From: ada@example.test\nTo: counsel@example.test\nSubject: ITIN {ITIN}\n\
         Message-ID: <form-1@example.test>\nDate: Mon, 06 Mar 2023 10:00:00 +0000\n\n\
         My SSN is {SSN}; the zebra form is attached.\n"
        );
        let inbound = format!(
            "From: counsel@example.test\nTo: ada@example.test\nSubject: filed\n\
         Message-ID: <in-1@example.test>\nDate: Tue, 07 Mar 2023 10:00:00 +0000\n\n\
         The trust's EIN: {EIN} is on the yak filing.\n"
        );
        let clean = "From: counsel@example.test\nTo: ada@example.test\nSubject: thanks\n\
                 Message-ID: <ok-1@example.test>\nDate: Wed, 08 Mar 2023 10:00:00 +0000\n\n\
                 Received with thanks, ibex 123456789.\n";
        std::fs::write(mail.join("ada/Sent/cur/1:2,S"), &sent).expect("sent");
        std::fs::write(mail.join("ada/Inbox/cur/2:2,S"), &inbound).expect("inbound");
        std::fs::write(mail.join("ada/Inbox/cur/3:2,S"), clean).expect("clean");
        crate::run::succeed(
            &Cmd::new("notmuch")
                .arg("new")
                .env("NOTMUCH_CONFIG", &config),
        )
        .expect("indexed");
        let roster = Roster::new(vec![MailAccount {
            email: suzatary_core::mail::account_email::AccountEmail::new("ada@example.test")
                .expect("a lower-case address"),
            subdir: "ada".to_owned(),
            sync: suzatary_core::mail::account::Sync::Gmail,
        }])
        .expect("a roster");
        let keyed = crate::state::Ledger::new(
            root.join("state.json"),
            root.join("local/mail.json"),
            || {
                Ok(suzatary_core::mail::dedup_key::DedupKey::of(
                    &suzatary_core::capability::Secret::new("12".repeat(32)),
                )
                .expect("a key"))
            },
        );
        let mailbox = Notmuch::new(&mail, keyed.clone(), roster).with_config(&config);
        let known = keyed.read().expect("no ledger is an empty one");
        (mailbox, known)
    }

    /// With the real notmuch, where it is on PATH: every reading a model or a page gets of a sent
    /// and an inbound message naming numbers holds placeholders and never a number, the index
    /// still finds the number (it is the box's own), a handle read off a placeholder reveals its
    /// value, and a clean message reads as it is.
    #[test]
    fn every_reading_holds_placeholders_and_a_handle_reveals_its_value() {
        if crate::run::run(&Cmd::new("notmuch").arg("--version")).is_err() {
            eprintln!("notmuch is not on PATH: the real-index half of held values is not run");
            return;
        }
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let (mailbox, known) = indexed(scratch.path());
        let query = |text: &str| Query::new(text).expect("a query");

        // The index is the box's own and still finds the number.
        assert_eq!(
            mailbox.newest_ids(SSN, 5).expect("a search"),
            vec!["form-1@example.test"]
        );

        let readings: Vec<String> = vec![
            serde_json::to_string(
                &mailbox
                    .search(&query("zebra or yak or ibex"), None, 10, None)
                    .expect("rows"),
            )
            .expect("JSON"),
            serde_json::to_string(&mailbox.message("form-1@example.test").expect("a message"))
                .expect("JSON"),
            serde_json::to_string(&mailbox.message("in-1@example.test").expect("a message"))
                .expect("JSON"),
            serde_json::to_string(&mailbox.thread("form-1@example.test", 10).ok()).expect("JSON"),
            serde_json::to_string(&mailbox.summaries("folder:ada/Sent", None).expect("rows"))
                .expect("JSON"),
            serde_json::to_string(
                &mailbox
                    .full_message("form-1@example.test", &known)
                    .expect("shown"),
            )
            .expect("JSON"),
            serde_json::to_string(
                &mailbox
                    .full_message("in-1@example.test", &known)
                    .expect("shown"),
            )
            .expect("JSON"),
        ];
        for reading in &readings {
            for number in [SSN, ITIN, EIN] {
                assert!(!reading.contains(number), "{number} in {reading}");
            }
        }
        let all = readings.join("\n");
        for placeholder in [
            "[SSN ending 3456 · tap to show · held:",
            "[ITIN ending 1234 · tap to show · held:",
            "[EIN ending 0001 · tap to show · held:",
        ] {
            assert!(all.contains(placeholder), "{placeholder} in {all}");
        }

        let handles: std::collections::BTreeSet<String> =
            regex::Regex::new(r"held:[A-Za-z0-9_-]+\.[0-9]+")
                .expect("a literal pattern")
                .find_iter(&all)
                .map(|found| found.as_str().to_owned())
                .collect();
        let revealed: std::collections::BTreeSet<String> = handles
            .iter()
            .map(|text| {
                match mailbox
                    .reveal(&Handle::parse(text).expect("a handle"))
                    .expect("read")
                {
                    Revealed::Value(shown) => shown.into_string(),
                    Revealed::Gone => panic!("{text} is gone"),
                }
            })
            .collect();
        assert_eq!(
            revealed,
            [SSN, ITIN, EIN].into_iter().map(str::to_owned).collect()
        );

        let shown = mailbox.message("ok-1@example.test").expect("the clean one");
        assert_eq!(
            shown.body.trim_end(),
            "Received with thanks, ibex 123456789."
        );
        assert_eq!(shown.row.subject, "thanks");
        for gone in [
            Handle::new("ok-1@example.test", 0),
            Handle::new("form-1@example.test", 9),
            Handle::new("no\"such@example.test", 0),
        ] {
            assert_eq!(
                mailbox.reveal(&gone.expect("a handle")).expect("read"),
                Revealed::Gone
            );
        }
    }

    /// A ledger no test here reads: its key never resolves.
    fn ledger() -> crate::state::Ledger {
        crate::state::Ledger::new(
            "/tmp/state.json".into(),
            "/tmp/local/mail.json".into(),
            || Err("no key in this test".to_owned()),
        )
    }

    /// One message seen on three accounts is one row with three sources, the first read its
    /// primary, and a place seen twice counts once.
    #[test]
    fn a_message_counts_each_place_it_was_seen_once() {
        let at = |account: &str| Source {
            account: account.to_owned(),
            container: "inbox".to_owned(),
            native_id: format!("{account}-1"),
            path: PathBuf::from(format!("/mail/{account}/cur/1")),
        };
        let mut message = CanonicalMessage {
            hash: "ab".repeat(32),
            sender: "ada@example.test".to_owned(),
            title: "Hello".to_owned(),
            timestamp: None,
            primary: at("one"),
            also: Vec::new(),
        };
        for source in [at("two"), at("one"), at("three"), at("two")] {
            message.add_source(source);
        }
        let row = Notmuch::summary_of(&message);
        assert_eq!(row.sources, 3);
        assert_eq!(row.id, "one-1");
    }

    fn roster(accounts: Vec<MailAccount>) -> Roster {
        Roster::new(accounts).expect("a roster")
    }

    #[test]
    fn by_id_query_drops_a_quoted_or_whitespace_id() {
        let ids = ["abc123", "has space", "has\"quote", "def456"];
        assert_eq!(by_id_query(ids), "id:\"abc123\" or id:\"def456\"");
        assert_eq!(by_id_query(std::iter::empty()), "");
    }

    #[test]
    fn a_configured_notmuch_carries_notmuch_config_as_an_explicit_env_var() {
        let nm = Notmuch::new("/tmp/mail", ledger(), roster(vec![]))
            .with_binary("notmuch")
            .with_config("/tmp/notmuch-config");
        let command = nm
            .command(&["search"])
            .expect("a configured Notmuch builds a command");
        assert_eq!(
            command
                .env
                .iter()
                .find(|(key, _)| key == "NOTMUCH_CONFIG")
                .and_then(|(_, value)| value.as_deref()),
            Some(std::ffi::OsStr::new("/tmp/notmuch-config")),
            "NOTMUCH_CONFIG must be the capability's own value, not left to the process environment"
        );
        assert!(
            command.pass.iter().all(|name| !name.starts_with("NOTMUCH")),
            "NOTMUCH_DATABASE from the process environment would name another index: {:?}",
            command.pass
        );
    }

    #[test]
    fn an_unconfigured_notmuch_refuses_a_search_without_spawning_anything() {
        // A nonexistent binary: the refusal must come from the missing config, not a spawn.
        let nm = Notmuch::new("/tmp/mail", ledger(), roster(vec![]))
            .with_binary("/definitely/not/a/real/notmuch/binary");
        let err = nm
            .command(&["search"])
            .expect_err("no config named means no command built");
        assert_eq!(
            err.to_string(),
            "the mail index could not be read: this deployment has no mail index"
        );
    }
}
