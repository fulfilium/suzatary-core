//! A Microsoft account's Junk folder, read-only: the interpreter of
//! `suzatary_core::mail::junk_folder::Junk`.
//!
//! The folder is `EXAMINE`d and fetched with `BODY.PEEK`, never selected, and the token is minted
//! silently or the read is [`JunkFailure::NeedsLogin`].

use std::io::{Read, Write};
use std::path::PathBuf;

use chrono::NaiveDate;
use mail_parser::{HeaderName, Message, MessageParser, PartType};
use suzatary_core::mail::MAX_BODY_CHARS;
use suzatary_core::mail::account::{MailAccount, Roster};
use suzatary_core::mail::imap::imap_date;
use suzatary_core::mail::junk::{ChurnList, JunkHeader, JunkMessage, Listed, Uid, junk_folder};
use suzatary_core::mail::junk_folder::{Junk, JunkFailure};
use suzatary_core::mail::oauth::{Imap, Silent};
use suzatary_core::spelling::clip;

use crate::imap::{IMAP_HOST, IMAP_PORT, TlsStream, authenticated};
use crate::oauth::TokenStore;

/// UIDs per header `FETCH`.
const FETCH_CHUNK: usize = 200;

/// Peeked, so nothing is marked seen.
const HEADERS: &str = "(UID BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)])";

const WHOLE: &str = "(UID BODY.PEEK[])";

/// How a session is opened for an account: authenticated, nothing selected.
pub trait Connect {
    type Stream: Read + Write;

    /// [`JunkFailure::NeedsLogin`] without a silent token.
    fn open(&self, account: &MailAccount) -> Result<imap::Session<Self::Stream>, JunkFailure>;
}

/// Microsoft's IMAP endpoint over TLS, with a token minted silently from the stored grant or not
/// at all.
#[derive(Debug, Clone)]
pub struct SignedIn {
    tokens: TokenStore,
}

impl SignedIn {
    #[must_use]
    pub fn new(tokens: TokenStore) -> Self {
        Self { tokens }
    }
}

impl Connect for SignedIn {
    type Stream = TlsStream;

    fn open(&self, account: &MailAccount) -> Result<imap::Session<TlsStream>, JunkFailure> {
        let token = match self
            .tokens
            .access::<Imap>(account)
            .map_err(|failed| unreachable(account, &failed))?
        {
            Silent::Token(token) => token,
            Silent::NeedsLogin(needs) => return Err(JunkFailure::NeedsLogin(needs)),
        };
        authenticated(IMAP_HOST, IMAP_PORT, account.email.as_str(), &token)
            .map_err(|failed| unreachable(account, &failed))
    }
}

/// The seed file as the derivation wrote it: one normalised domain per line.
fn read_seed(path: &std::path::Path) -> std::io::Result<Vec<String>> {
    let text = std::fs::read_to_string(path)?;
    Ok(text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Every Microsoft account's Junk, through one [`Connect`].
#[derive(Debug, Clone)]
pub struct OutlookJunk<C> {
    accounts: Vec<MailAccount>,
    churn: ChurnList,
    seed_path: Option<PathBuf>,
    connect: C,
}

impl<C: Connect> OutlookJunk<C> {
    #[must_use]
    pub fn new(roster: &Roster, churn: ChurnList, seed_path: Option<PathBuf>, connect: C) -> Self {
        Self {
            accounts: roster
                .accounts()
                .iter()
                .filter(|account| account.microsoft().is_some())
                .cloned()
                .collect(),
            churn,
            seed_path,
            connect,
        }
    }

    fn account(&self, email: &str) -> Result<&MailAccount, JunkFailure> {
        self.accounts
            .iter()
            .find(|account| account.email.as_str() == email)
            .ok_or_else(|| JunkFailure::Unknown(email.to_owned()))
    }

    fn reading<T>(
        &self,
        email: &str,
        read: impl FnOnce(&mut Examined<C::Stream>) -> Result<T, JunkFailure>,
    ) -> Result<T, JunkFailure> {
        let account = self.account(email)?;
        let mut examined = Examined::open(self.connect.open(account)?, account)?;
        let answer = read(&mut examined);
        examined.logout();
        answer
    }
}

impl<C: Connect + Sync> Junk for OutlookJunk<C> {
    fn junk_accounts(&self) -> Vec<String> {
        self.accounts
            .iter()
            .map(|account| account.email.to_string())
            .collect()
    }

    fn churn(&self) -> ChurnList {
        // Read on every digest so a rebuilt seed needs no restart.
        let Some(path) = &self.seed_path else {
            return self.churn.clone();
        };
        match read_seed(path) {
            Ok(lines) => self
                .churn
                .clone()
                .with_seed(lines.iter().map(String::as_str))
                .expect("suzatary-junk-seed normalises every line to a domain"),
            Err(_) => self.churn.clone(),
        }
    }

    fn junk_headers(
        &self,
        account: &str,
        since: NaiveDate,
    ) -> Result<Vec<JunkHeader>, JunkFailure> {
        self.reading(account, |examined| examined.headers(since))
    }

    fn junk_message(&self, account: &str, uid: Uid) -> Result<JunkMessage, JunkFailure> {
        self.reading(account, |examined| examined.message(uid))
    }
}

/// A session whose Junk folder is examined; these methods are all that reach it.
struct Examined<S: Read + Write> {
    session: imap::Session<S>,
    account: MailAccount,
}

impl<S: Read + Write> Examined<S> {
    fn open(mut session: imap::Session<S>, account: &MailAccount) -> Result<Self, JunkFailure> {
        let names = session
            .list(Some(""), Some("*"))
            .map_err(|failed| unreachable(account, &failed))?;
        let listed: Vec<Listed> = names
            .iter()
            .map(|name| Listed {
                name: name.name().to_owned(),
                // `imap` does not re-export `NameAttribute`, so its Debug name stands in, pinned
                // by the containment test.
                junk: name
                    .attributes()
                    .iter()
                    .any(|attribute| format!("{attribute:?}") == "Junk"),
            })
            .collect();
        let folder = junk_folder(&listed)
            .ok_or_else(|| JunkFailure::NoJunkFolder(account.email.to_string()))?
            .to_owned();
        // Raw, because the crate quotes it.
        session
            .examine(&folder)
            .map_err(|failed| unreachable(account, &failed))?;
        Ok(Self {
            session,
            account: account.clone(),
        })
    }

    fn headers(&mut self, since: NaiveDate) -> Result<Vec<JunkHeader>, JunkFailure> {
        let mut uids: Vec<u32> = self
            .session
            .uid_search(format!("SINCE {}", imap_date(since)))
            .map_err(|failed| unreachable(&self.account, &failed))?
            .into_iter()
            .collect();
        uids.sort_unstable();
        let mut headers = Vec::with_capacity(uids.len());
        for chunk in uids.chunks(FETCH_CHUNK) {
            let set: Vec<String> = chunk.iter().map(u32::to_string).collect();
            let fetches = self
                .session
                .uid_fetch(set.join(","), HEADERS)
                .map_err(|failed| unreachable(&self.account, &failed))?;
            for fetch in fetches.iter() {
                let uid = fetch.uid.ok_or_else(|| JunkFailure::Unreachable {
                    account: self.account.email.to_string(),
                    why: "the server answered a UID FETCH without a UID".to_owned(),
                })?;
                headers.push(header_of(Uid(uid), fetch.header().unwrap_or_default()));
            }
        }
        Ok(headers)
    }

    fn message(&mut self, uid: Uid) -> Result<JunkMessage, JunkFailure> {
        let fetches = self
            .session
            .uid_fetch(uid.0.to_string(), WHOLE)
            .map_err(|failed| unreachable(&self.account, &failed))?;
        let raw = fetches
            .iter()
            .find(|fetch| fetch.uid == Some(uid.0))
            .and_then(imap::types::Fetch::body)
            .ok_or_else(|| JunkFailure::Missing {
                account: self.account.email.to_string(),
                uid: uid.0,
            })?;
        Ok(message_of(self.account.email.as_str(), uid, raw))
    }

    /// A close the server fumbles changes nothing already read.
    fn logout(mut self) {
        let _ = self.session.logout();
    }
}

fn unreachable(account: &MailAccount, failed: &impl std::fmt::Display) -> JunkFailure {
    JunkFailure::Unreachable {
        account: account.email.to_string(),
        why: failed.to_string(),
    }
}

fn header_of(uid: Uid, bytes: &[u8]) -> JunkHeader {
    let parsed = MessageParser::default().parse(bytes);
    header_from(uid, parsed.as_ref())
}

fn header_from(uid: Uid, parsed: Option<&Message<'_>>) -> JunkHeader {
    let first = parsed.and_then(Message::from).and_then(|from| from.first());
    JunkHeader::new(
        uid,
        first.and_then(|addr| addr.name()),
        first.and_then(|addr| addr.address()),
        parsed.and_then(Message::subject).unwrap_or_default(),
        parsed
            .and_then(|message| message.header_raw(HeaderName::Date))
            .unwrap_or_default(),
    )
}

/// A whole message as a [`JunkMessage`], its plain body or its HTML as text, cut at
/// [`MAX_BODY_CHARS`].
fn message_of(account: &str, uid: Uid, raw: &[u8]) -> JunkMessage {
    let parsed = MessageParser::default().parse(raw);
    let parts = |ids: &[u32], plain: bool| -> String {
        let message = parsed.as_ref();
        ids.iter()
            .filter_map(|id| message.and_then(|m| m.part(*id)))
            .filter_map(|part| match (&part.body, plain) {
                (PartType::Text(text), true) | (PartType::Html(text), false) => Some(text.as_ref()),
                _ => None,
            })
            .collect()
    };
    let plain = parsed
        .as_ref()
        .map(|message| parts(&message.text_body, true))
        .unwrap_or_default();
    let text = if plain.trim().is_empty() {
        let html = parsed
            .as_ref()
            .map(|message| parts(&message.html_body, false))
            .unwrap_or_default();
        crate::mailbox::text::html_to_text(&html)
    } else {
        plain.trim().to_owned()
    };
    let (body, truncated) = clip(&text, MAX_BODY_CHARS);
    JunkMessage {
        account: account.to_owned(),
        header: header_from(uid, parsed.as_ref()),
        body,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::{Arc, Mutex};

    use suzatary_core::cast::CAST;
    use suzatary_core::mail::account::{Authority, MsImap, Sync};

    use super::*;

    const ALLOWED: [&str; 6] = [
        "LOGIN",
        "LIST",
        "EXAMINE",
        "UID SEARCH",
        "UID FETCH",
        "LOGOUT",
    ];

    /// The commands that would change a mailbox, or open one writable.
    const FORBIDDEN: [&str; 9] = [
        "STORE", "COPY", "MOVE", "EXPUNGE", "APPEND", "DELETE", "SELECT", "CREATE", "RENAME",
    ];

    struct Held {
        header: String,
        raw: String,
    }

    fn held(name: &str, address: &str, subject: &str, body: &str) -> Held {
        let header = format!(
            "From: {name} <{address}>\r\nSubject: {subject}\r\nDate: Fri, 25 Sep 2026 10:00:00 +0000\r\n\r\n"
        );
        Held {
            raw: format!("{header}{body}\r\n"),
            header,
        }
    }

    /// A scripted IMAP server under a real `imap::Client`, logging each command and answering
    /// unknown ones `BAD`.
    struct Scripted {
        log: Arc<Mutex<Vec<String>>>,
        folders: Vec<(&'static str, &'static str)>,
        junk: BTreeMap<u32, Held>,
        written: Vec<u8>,
        answer: VecDeque<u8>,
    }

    impl Scripted {
        fn new(
            log: Arc<Mutex<Vec<String>>>,
            folders: Vec<(&'static str, &'static str)>,
            junk: BTreeMap<u32, Held>,
        ) -> Self {
            Self {
                log,
                folders,
                junk,
                written: Vec::new(),
                answer: b"* OK scripted IMAP ready\r\n".iter().copied().collect(),
            }
        }

        fn say(&mut self, text: &str) {
            self.answer.extend(text.as_bytes());
        }

        fn fetched(&self, set: &str) -> Vec<u32> {
            set.split(',')
                .filter_map(|uid| uid.parse().ok())
                .filter(|uid| self.junk.contains_key(uid))
                .collect()
        }

        fn command(&mut self, line: &str) {
            let (tag, command) = line.split_once(' ').expect("a tagged command");
            self.log
                .lock()
                .expect("the log lock")
                .push(command.to_owned());
            let ok = format!("{tag} OK done\r\n");
            if command.starts_with("LOGIN ") {
                self.say(&ok);
            } else if command.starts_with("LIST ") {
                for (attributes, name) in self.folders.clone() {
                    self.say(&format!("* LIST ({attributes}) \"/\" \"{name}\"\r\n"));
                }
                self.say(&ok);
            } else if command.starts_with("EXAMINE ") {
                let exists = self.junk.len();
                self.say(&format!(
                    "* {exists} EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n{tag} OK [READ-ONLY] done\r\n"
                ));
            } else if command.starts_with("UID SEARCH SINCE ") {
                let uids: Vec<String> = self.junk.keys().map(u32::to_string).collect();
                self.say(&format!("* SEARCH {}\r\n{ok}", uids.join(" ")));
            } else if let Some(rest) = command.strip_prefix("UID FETCH ") {
                let (set, what) = rest.split_once(' ').expect("a set and a query");
                for (seq, uid) in self.fetched(set).into_iter().enumerate() {
                    let held = &self.junk[&uid];
                    let (section, bytes) = if what.contains("HEADER.FIELDS") {
                        (
                            "BODY[HEADER.FIELDS (FROM SUBJECT DATE)]",
                            held.header.clone(),
                        )
                    } else {
                        ("BODY[]", held.raw.clone())
                    };
                    self.say(&format!(
                        "* {} FETCH (UID {uid} {section} {{{}}}\r\n{bytes})\r\n",
                        seq + 1,
                        bytes.len()
                    ));
                }
                self.say(&ok);
            } else if command == "LOGOUT" {
                self.say(&format!("* BYE\r\n{ok}"));
            } else {
                self.say(&format!("{tag} BAD not scripted\r\n"));
            }
        }
    }

    impl Write for Scripted {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buf);
            while let Some(end) = self.written.windows(2).position(|w| w == b"\r\n") {
                let line: Vec<u8> = self.written.drain(..end + 2).collect();
                let line = String::from_utf8(line).expect("a command is text");
                self.command(line.trim_end());
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Read for Scripted {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = buf.len().min(self.answer.len());
            for (slot, byte) in buf.iter_mut().zip(self.answer.drain(..n)) {
                *slot = byte;
            }
            Ok(n)
        }
    }

    struct Fake {
        log: Arc<Mutex<Vec<String>>>,
        folders: Vec<(&'static str, &'static str)>,
        junk: fn() -> BTreeMap<u32, Held>,
    }

    impl Connect for Fake {
        type Stream = Scripted;

        fn open(&self, _account: &MailAccount) -> Result<imap::Session<Scripted>, JunkFailure> {
            let server = Scripted::new(self.log.clone(), self.folders.clone(), (self.junk)());
            let mut client = imap::Client::new(server);
            client.read_greeting().expect("a greeting");
            Ok(client
                .login("fake", "fake")
                .map_err(|(e, _)| e)
                .expect("a login"))
        }
    }

    fn roster() -> Roster {
        Roster::new(vec![
            MailAccount {
                email: suzatary_core::mail::account_email::AccountEmail::new(CAST.owner.email)
                    .expect("a lower-case address"),
                subdir: "owner".to_owned(),
                sync: Sync::MsImap(MsImap {
                    authority: Authority::Consumers,
                    inbox_folders: suzatary_core::mail::folders::Folders::inbox(),
                    sent_folder: "Sent".to_owned(),
                }),
            },
            MailAccount {
                email: suzatary_core::mail::account_email::AccountEmail::new(CAST.accountant.email)
                    .expect("a lower-case address"),
                subdir: "gmail".to_owned(),
                sync: Sync::Gmail,
            },
        ])
        .expect("a roster")
    }

    fn scene() -> BTreeMap<u32, Held> {
        BTreeMap::from([
            (
                4,
                held("Deals", "deals@offers.example.net", "70% off", "buy"),
            ),
            (
                9,
                held(
                    CAST.friend.name,
                    CAST.friend.email,
                    "RSVP for the wedding",
                    "Will you come?",
                ),
            ),
            (
                12,
                held("Deals", "deals@offers.example.net", "80% off", "buy"),
            ),
        ])
    }

    fn junk(
        folders: Vec<(&'static str, &'static str)>,
    ) -> (OutlookJunk<Fake>, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            log: log.clone(),
            folders,
            junk: scene,
        };
        let churn = ChurnList::new(
            ["offers.example.net"],
            suzatary_core::mail::junk::KeepList::default(),
        )
        .expect("a list");
        (OutlookJunk::new(&roster(), churn, None, fake), log)
    }

    fn since() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 28).expect("a date")
    }

    #[test]
    fn a_digest_and_a_show_send_nothing_that_could_change_the_mailbox() {
        let (junk, log) = junk(vec![
            ("\\HasNoChildren", "Inbox"),
            ("\\HasNoChildren", "Junk"),
            ("\\HasNoChildren \\Junk", "Junk Email"),
        ]);
        let headers = junk
            .junk_headers(CAST.owner.email, since())
            .expect("headers");
        assert_eq!(headers.len(), 3);
        let shown = junk
            .junk_message(CAST.owner.email, Uid(9))
            .expect("a message");
        assert_eq!(shown.body, "Will you come?");

        let sent = log.lock().expect("the log lock").clone();
        assert!(!sent.is_empty());
        for command in &sent {
            assert!(
                ALLOWED.iter().any(|verb| command.starts_with(verb)),
                "an unexpected command reached the wire: {command}"
            );
            let verb: String = command
                .split(' ')
                .take_while(|word| word.chars().all(|c| c.is_ascii_uppercase()))
                .collect::<Vec<_>>()
                .join(" ");
            for forbidden in FORBIDDEN {
                assert!(
                    !verb.split(' ').any(|word| word == forbidden),
                    "{forbidden} reached the wire: {command}"
                );
            }
            if command.starts_with("UID FETCH") {
                assert!(
                    command.contains("BODY.PEEK["),
                    "a fetch that marks seen: {command}"
                );
            }
        }
        assert_eq!(
            sent.iter()
                .filter(|command| command.as_str() == "EXAMINE \"Junk Email\"")
                .count(),
            2,
            "{sent:?}"
        );
        assert!(
            sent.iter()
                .any(|command| command == "UID SEARCH SINCE 28-Aug-2026")
        );
        assert_eq!(
            sent.iter()
                .filter(|command| command.as_str() == "LOGOUT")
                .count(),
            2
        );
    }

    #[test]
    fn a_missing_grant_is_needs_login_and_nothing_is_dialled() {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-io-junk-no-grant-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let junk = OutlookJunk::new(
            &roster(),
            ChurnList::default(),
            None,
            SignedIn::new(TokenStore::new(&dir)),
        );
        let failed = junk
            .junk_headers(CAST.owner.email, since())
            .expect_err("no grant");
        let JunkFailure::NeedsLogin(needs) = &failed else {
            panic!("not a login to ask for: {failed:?}");
        };
        assert!(
            needs
                .to_string()
                .contains(&format!("suzatary auth {} --scope imap", CAST.owner.email)),
            "{needs}"
        );
        assert!(!dir.exists(), "reading a grant wrote something");
    }
}
