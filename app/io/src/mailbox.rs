//! notmuch and the maildir, read only, as [`Mailbox`].
//!
//! Every list asks the index for the newest ids first and only loads those: loading hashes
//! every file a query matches, and notmuch has no LIMIT.

pub mod attachments;
pub mod dedup;
pub mod forwarded;
pub mod held;
pub mod located;
pub mod message;
pub mod text;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::present::Present as _;
use crate::run::{Cmd, succeed};

/// What notmuch reads of its environment beyond [`crate::run::BASE`].
const NOTMUCH: &[&str] = &["NOTMUCH_*"];

/// `cmd` told which index to use: the `config` named and no other `NOTMUCH_*` variable (which
/// could point it at another index); with none named, the environment's.
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
use suzatary_core::held_value::HeldFile;
use suzatary_core::held_value::{self, Reopened, Reveal};
use suzatary_core::mail::account::{MailAccount, Roster};
use suzatary_core::mail::hash_prefix::{Ambiguous, HashPrefix, Resolved};
use suzatary_core::mail::known::Known;
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
    ledger: crate::state::Ledger,
    /// Where each listed hash lives: every listing writes it, every lookup by hash reads it.
    located: located::Located,
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
    #[must_use]
    pub fn new(
        mail_root: impl Into<PathBuf>,
        ledger: crate::state::Ledger,
        located: located::Located,
        accounts: Roster,
    ) -> Self {
        Self {
            binary: PathBuf::from("notmuch"),
            mail_root: mail_root.into(),
            ledger,
            located,
            accounts,
            config: None,
            pdftotext: crate::pdftotext::Pdftotext::new("."),
        }
    }

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

    /// Refuses, spawning nothing, when no config is named.
    fn command(&self, args: &[&str]) -> Result<Cmd, MailError> {
        let Some(config) = &self.config else {
            return Err(MailError::Unreachable(
                "this deployment has no mail index".to_owned(),
            ));
        };
        Ok(indexed_by(Cmd::new(&self.binary).args(args), Some(config)))
    }

    /// Raw bytes: notmuch copies a message's bytes into its JSON unchanged, so the answer need
    /// not be UTF-8; each reader decodes it (`message::Text`).
    fn run(&self, args: &[&str]) -> Result<Vec<u8>, MailError> {
        Ok(succeed(&self.command(args)?)
            .map_err(|e| MailError::Unreachable(e.to_string()))?
            .stdout)
    }

    /// From the index alone.
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

    fn thread_ids(&self, query: &str) -> Result<Vec<String>, MailError> {
        let raw = self.run(&["search", "--format=json", "--output=threads", query])?;
        parse_id_array(&raw)
    }

    /// Every message `query` matches, folded by content hash, each hash recorded in
    /// [`located::Located`] before it is answered so every printed hash resolves. It reads and
    /// hashes each file, so page reads bound `query` through [`Self::newest_ids`] first.
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
            let holder = held_value::Holder::Message(msg.id.clone());
            let (sender, title) = (
                held_value::mask(&sender, &holder, &values),
                held_value::mask(&title, &holder, &values),
            );
            let held = values.kinds();
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
                    held,
                    primary: source,
                    also: Vec::new(),
                });
        }
        for message in messages.values() {
            self.located
                .record(&message.hash, &message.primary.native_id)?;
        }
        Ok(messages)
    }

    /// `--include-html` is required: without it an html-only message arrives empty.
    fn show_query(&self, query: &str) -> Result<Vec<Shown>, MailError> {
        let raw = self.run(&["show", "--format=json", "--include-html", query])?;
        Ok(message::matched(&raw)?.into_iter().map(shown_of).collect())
    }

    /// Unquoted because the id is notmuch's own.
    fn show_by_native_id(&self, id: &str) -> Result<Option<Shown>, MailError> {
        Ok(self.show_query(&format!("id:{id}"))?.into_iter().next())
    }

    /// The notmuch id recorded for the one listed hash `prefix` begins.
    pub fn locate(&self, prefix: &HashPrefix) -> Result<Resolved<String>, MailError> {
        self.located.resolve(prefix)
    }

    /// By notmuch id, or by a listed hash's prefix.
    fn shown_by(&self, id: &str) -> Result<Lookup, MailError> {
        let shown = |found: Option<Shown>| found.map_or(Lookup::Absent, Lookup::Shown);
        if let Some(found) = self.show_by_native_id(id)? {
            return Ok(Lookup::Shown(found));
        }
        let Some(prefix) = HashPrefix::parse(id) else {
            return Ok(Lookup::Absent);
        };
        Ok(match self.locate(&prefix)? {
            Resolved::Unlisted => Lookup::Absent,
            Resolved::One(_, native) => shown(self.show_by_native_id(&native)?),
            Resolved::Ambiguous(ambiguous) => Lookup::Ambiguous(ambiguous),
        })
    }

    /// [`Self::shown_by`], an ambiguous prefix a query error.
    fn resolve_show(&self, id: &str) -> Result<Option<Shown>, MailError> {
        match self.shown_by(id)? {
            Lookup::Shown(shown) => Ok(Some(shown)),
            Lookup::Absent => Ok(None),
            Lookup::Ambiguous(ambiguous) => Err(MailError::Query(ambiguous.to_string())),
        }
    }

    /// Read on every call because other processes write it; `None` on a device with no key,
    /// whose rows say so rather than fail.
    fn load_state(&self) -> Result<Option<Known>, MailError> {
        self.ledger.read_if_keyed().map_err(MailError::Unreachable)
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

    fn rows(&self, query: &str, limit: usize) -> Result<Vec<MailRow>, MailError> {
        let messages = self.load_messages(query)?;
        let state = self.load_state()?;
        let mut ordered: Vec<&CanonicalMessage> = messages.values().collect();
        ordered.sort_by_key(|m| std::cmp::Reverse(epoch_of(m)));
        ordered.truncate(limit);
        Ok(ordered
            .into_iter()
            .map(|m| row_of(m, state.as_ref()))
            .collect())
    }
}

impl Notmuch {
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
            held: message.held,
        }
    }

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

    /// Any notmuch query as the agent's summaries, newest first. Unlike [`Mailbox::search`] it
    /// loads every match and then truncates.
    pub fn summaries(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<verbs::Summary>, MailError> {
        Ok(Self::newest_first(self.load_messages(query)?, limit))
    }

    /// The triage queue: inbox mail whose hash `state.json` has not seen. Gated on the hash, not
    /// the unread flag; the 14-day floor only bounds a re-tag flood (the fetch window is 7 days).
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

    /// One message in full, by notmuch id or by canonical hash (bridged through the listed
    /// hashes' locators, since notmuch indexes only `Message-ID`s).
    pub fn full_message(&self, id: &str) -> Result<verbs::Show, MailError> {
        Ok(match self.shown_by(id)? {
            Lookup::Shown(shown) => verbs::Show::Shown(Self::message_of(shown)),
            Lookup::Absent => verbs::Show::NotFound { id: id.to_owned() },
            Lookup::Ambiguous(ambiguous) => verbs::Show::Ambiguous(ambiguous),
        })
    }

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

    /// Hash every message the index holds and record where each lives; answers how many. It
    /// reads every file once.
    pub fn locate_all(&self) -> Result<usize, MailError> {
        Ok(self.load_messages("*")?.len())
    }
}

/// A row plus what a page needs: thread, whitelisted headers, clipped body, filtered links.
fn full(
    message: &CanonicalMessage,
    shown: Option<&Shown>,
    state: Option<&Known>,
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
    /// The kinds of held value the mask counted in its file.
    held: held_value::Kinds,
    primary: Source,
    also: Vec<Source>,
}

impl CanonicalMessage {
    fn add_source(&mut self, source: Source) {
        if self.primary != source && !self.also.contains(&source) {
            self.also.push(source);
        }
    }

    fn sources(&self) -> usize {
        1 + self.also.len()
    }
}

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

fn row_of(message: &CanonicalMessage, state: Option<&Known>) -> MailRow {
    let disposition = state.map_or(suzatary_core::mail::Disposed::Unkeyed, |state| {
        suzatary_core::mail::Disposed::from(state.decided(&message.hash))
    });
    let primary = &message.primary;
    MailRow {
        hash: message.hash.clone(),
        id: primary.native_id.clone(),
        sender: message.sender.clone(),
        subject: message.title.clone(),
        date: message.timestamp.and_then(|p| p.to_naive_datetime()),
        container: primary.container.clone(),
        account: primary.account.clone(),
        disposition,
        held: message.held,
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

/// What a notmuch id or a hash prefix finds.
enum Lookup {
    Shown(Shown),
    /// Also when a listed hash matches and notmuch no longer holds its message.
    Absent,
    /// A prefix more than one listed hash begins with.
    Ambiguous(Ambiguous),
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
    let holder = held_value::Holder::Message(shown.id.clone());
    let mask = |text: &str| held_value::mask(text, &holder, &values);
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

fn scoped_query(query: &str, account: Option<&MailAccount>) -> String {
    match account {
        None => query.to_owned(),
        Some(a) => format!("({query}) and ({})", a.query()),
    }
}

fn inbox_query(account: Option<&MailAccount>) -> String {
    scoped_query("tag:inbox", account)
}

/// `query`, bounded to strictly before the page cursor. The cursor is read as UTC because the
/// wire type has no zone, which can skip or repeat one row at a page boundary.
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
    fn value(&self, message: &str, ordinal: u16) -> Result<Option<held_value::Shown>, MailError> {
        let query = by_id_query(std::iter::once(message));
        if query.is_empty() {
            return Ok(None);
        }
        let raw = self.run(&["show", "--format=json", "--include-html", &query])?;
        let Some(msg) = message::matched(&raw)?.into_iter().next() else {
            return Ok(None);
        };
        let (_, values) = unmasked(msg);
        Ok(values
            .get(ordinal)
            .map(|(kind, value)| held_value::Shown::new(kind, value)))
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
        Ok(full(message, Some(&shown), state.as_ref(), thread))
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
                full(
                    m,
                    by_native.get(native).copied(),
                    state.as_ref(),
                    thread.to_owned(),
                )
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
        let source = HeldFile::Mail {
            message: id.to_owned(),
            name: name.to_owned(),
        };
        read_of(&source, name, &file.mime, file.content, &self.pdftotext)
            .map_err(MailError::Unreachable)
    }
}

/// A file read for the model, a PDF with its `pdftotext` layer; `Err` is a `pdftotext` failure,
/// a fact about the box, not the file.
pub(crate) fn read_of(
    file: &HeldFile,
    name: &str,
    mime: &str,
    content: Vec<u8>,
    pdftotext: &crate::pdftotext::Pdftotext,
) -> Result<FileRead, String> {
    let layer = match Enclosure::sniff(&content) {
        Some(Enclosure::Pdf) => Some(pdftotext.text(&content)?),
        Some(Enclosure::Image(_)) | None => None,
    };
    Ok(FileRead::of(file, name, mime, content, layer.as_deref()))
}

/// A maildir indexed by the real notmuch, for the tests of this module and of the corpus.
#[cfg(test)]
pub(crate) mod fixture {
    use super::{Cmd, MailAccount, Notmuch, Roster, located};

    /// `files` written under `root/Mail` and indexed by the real notmuch under their own config,
    /// new mail tagged `tags` (`;`-separated), account `ada@example.test` reading `ada/`.
    pub(crate) fn indexed(
        root: &std::path::Path,
        tags: &str,
        files: &[(&str, String)],
    ) -> (Notmuch, crate::state::Ledger) {
        let mail = root.join("Mail");
        for leaf in ["ada/Inbox/cur", "ada/Inbox/new", "ada/Inbox/tmp"] {
            std::fs::create_dir_all(mail.join(leaf)).expect("a leaf");
        }
        for (path, text) in files {
            let path = mail.join(path);
            std::fs::create_dir_all(path.parent().expect("a file in a folder")).expect("a leaf");
            std::fs::write(&path, text).expect("a message");
        }
        let config = root.join("notmuch-config");
        std::fs::write(
            &config,
            format!(
                "[database]\npath={}\nhook_dir={}\n[user]\nname=Ada\n\
                 primary_email=ada@example.test\n[new]\ntags={tags}\n",
                mail.display(),
                root.join("hooks").display()
            ),
        )
        .expect("a notmuch config");
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
        let mailbox = Notmuch::new(
            &mail,
            keyed.clone(),
            located::Located::open(root.join("located")),
            roster,
        )
        .with_config(&config);
        (mailbox, keyed)
    }

    /// Whether the real notmuch is on PATH; a test that needs it says so when it is not.
    pub(crate) fn notmuch_present(test: &str) -> bool {
        let present = crate::run::run(&Cmd::new("notmuch").arg("--version")).is_ok();
        if !present {
            eprintln!("notmuch is not on PATH: {test} is not run");
        }
        present
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::held_value::{Handle, Holder};
    use suzatary_core::redact::Kind;

    /// Synthetic numbers no person holds: SSN area 000, ITIN area 912, an EIN of zeros.
    const SSN: &str = "000-12-3456";
    const ITIN: &str = "912-70-1234";
    const EIN: &str = "00-0000001";
    const PASSPORT: &str = "X12345678";
    const A_NUMBER: &str = "A-012-345-678";

    /// A sent, an inbound and a clean message under `root`.
    fn indexed(root: &std::path::Path) -> Notmuch {
        let sent = format!(
            "From: ada@example.test\nTo: counsel@example.test\nSubject: ITIN {ITIN}\n\
         Message-ID: <form-1@example.test>\nDate: Mon, 06 Mar 2023 10:00:00 +0000\n\n\
         My SSN is {SSN}; the zebra form is attached.\n"
        );
        let inbound = format!(
            "From: counsel@example.test\nTo: ada@example.test\nSubject: filed\n\
         Message-ID: <in-1@example.test>\nDate: Tue, 07 Mar 2023 10:00:00 +0000\n\n\
         The trust's EIN: {EIN} is on the yak filing.\n\
         Passport No. {PASSPORT}, A-Number {A_NUMBER}.\n"
        );
        let clean = "From: counsel@example.test\nTo: ada@example.test\nSubject: thanks\n\
                 Message-ID: <ok-1@example.test>\nDate: Wed, 08 Mar 2023 10:00:00 +0000\n\n\
                 Received with thanks, ibex 123456789.\n";
        fixture::indexed(
            root,
            "new",
            &[
                ("ada/Sent/cur/1:2,S", sent),
                ("ada/Inbox/cur/2:2,S", inbound),
                ("ada/Inbox/cur/3:2,S", clean.to_owned()),
            ],
        )
        .0
    }

    /// Every reading holds placeholders and never a number, the box's own index still finds the
    /// number, a handle reveals its value, and a clean message reads as it is.
    #[test]
    fn every_reading_holds_placeholders_and_a_handle_reveals_its_value() {
        if !fixture::notmuch_present("the real-index half of held values") {
            return;
        }
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let mailbox = indexed(scratch.path());
        let query = |text: &str| Query::new(text).expect("a query");

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
            serde_json::to_string(&mailbox.full_message("form-1@example.test").expect("shown"))
                .expect("JSON"),
            serde_json::to_string(&mailbox.full_message("in-1@example.test").expect("shown"))
                .expect("JSON"),
        ];
        for reading in &readings {
            for number in [SSN, ITIN, EIN, PASSPORT, A_NUMBER, "012345678"] {
                assert!(!reading.contains(number), "{number} in {reading}");
            }
        }
        let all = readings.join("\n");
        for placeholder in [
            "[SSN ending 3456 · tap to show · held:",
            "[ITIN ending 1234 · tap to show · held:",
            "[EIN ending 0001 · tap to show · held:",
            "[passport · tap to show · held:",
            "[A-number ending 5678 · tap to show · held:",
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
                let Ok(Handle::Value(place)) = Handle::parse(text) else {
                    panic!("{text} is no value's handle");
                };
                let Holder::Message(message) = place.holder() else {
                    panic!("{text} names no message");
                };
                mailbox
                    .value(message, place.ordinal())
                    .expect("read")
                    .unwrap_or_else(|| panic!("{text} is gone"))
                    .into_string()
            })
            .collect();
        assert_eq!(
            revealed,
            [SSN, ITIN, EIN, PASSPORT, A_NUMBER]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );

        let shown = mailbox.message("ok-1@example.test").expect("the clean one");
        assert_eq!(
            shown.body.trim_end(),
            "Received with thanks, ibex 123456789."
        );
        assert_eq!(shown.row.subject, "thanks");
        for (message, ordinal) in [
            ("ok-1@example.test", 0),
            ("form-1@example.test", 9),
            ("no\"such@example.test", 0),
        ] {
            assert_eq!(mailbox.value(message, ordinal).expect("read"), None);
        }
    }

    /// A message no listing has printed is not found by its hash until `locate_all`.
    #[test]
    fn locating_all_records_a_hash_no_listing_printed() {
        if !fixture::notmuch_present("the rebuild of the locators") {
            return;
        }
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let text = "From: counsel@example.test\nTo: ada@example.test\nSubject: cited\n\
                    Message-ID: <cited-1@example.test>\nDate: Wed, 08 Mar 2023 10:00:00 +0000\n\n\
                    Cited by a todo.\n";
        let (mailbox, _) = fixture::indexed(
            scratch.path(),
            "new",
            &[("ada/Inbox/cur/1:2,S", text.to_owned())],
        );
        let hash = dedup::canonical_hash(text.as_bytes());
        assert_eq!(
            mailbox.full_message(&hash).expect("answers"),
            verbs::Show::NotFound { id: hash.clone() }
        );
        assert_eq!(mailbox.locate_all().expect("located"), 1);
        let verbs::Show::Shown(shown) = mailbox.full_message(&hash[..8]).expect("answers") else {
            panic!("the cited message is found once located");
        };
        assert_eq!(shown.id, "cited-1@example.test");
    }

    /// A search row and the agent's row name the kinds of held value and hold none of them.
    #[test]
    fn a_search_row_names_the_kinds_its_message_holds_and_no_value() {
        if crate::run::run(&Cmd::new("notmuch").arg("--version")).is_err() {
            eprintln!("notmuch is not on PATH: the real-index half of held values is not run");
            return;
        }
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let mailbox = indexed(scratch.path());
        let query = Query::new("zebra or yak or ibex").expect("a query");
        let rows = mailbox.search(&query, None, 10, None).expect("rows");
        let kinds: Vec<(&str, Vec<Kind>)> = rows
            .iter()
            .map(|row| (row.id.as_str(), row.held.iter().collect()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("ok-1@example.test", vec![]),
                (
                    "in-1@example.test",
                    vec![Kind::Ein, Kind::Passport, Kind::ANumber]
                ),
                ("form-1@example.test", vec![Kind::Ssn, Kind::Itin]),
            ]
        );
        let summaries = mailbox
            .summaries("zebra or yak or ibex", None)
            .expect("rows");
        assert_eq!(summaries.len(), rows.len());
        for summary in &summaries {
            assert!(
                rows.iter()
                    .any(|row| row.id == summary.id && row.held == summary.held),
                "{summary:?}"
            );
        }
        let printed = format!(
            "{}{}",
            serde_json::to_string(&rows).expect("JSON"),
            serde_json::to_string(&summaries).expect("JSON")
        );
        for number in [SSN, ITIN, EIN, PASSPORT, A_NUMBER, "012345678"] {
            assert!(!printed.contains(number), "{number} in {printed}");
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
            held: held_value::Kinds::empty(),
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
        let nm = Notmuch::new(
            "/tmp/mail",
            ledger(),
            located::Located::open("/tmp/located"),
            roster(vec![]),
        )
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
        let nm = Notmuch::new(
            "/tmp/mail",
            ledger(),
            located::Located::open("/tmp/located"),
            roster(vec![]),
        )
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
