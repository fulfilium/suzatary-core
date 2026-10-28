//! Searching each account's mail server for mail older than the routine sync's window: the
//! interpreter of `suzatary_core::capability::OlderMail`.
//!
//! - A Microsoft account is `EXAMINE`d over the same IMAP session a backfill pulls through
//!   (`crate::imap::Outlook`), searched with `suzatary_core::mail::older::imap_criteria`, and
//!   pulled by `suzatary_core::mail::older::pull` into `<subdir>/older-inbox`, with a state
//!   file of its own beside the routine pull's.
//! - A Gmail account is searched through the Gmail API on lieer's grant (`crate::gmail`), or, for
//!   a Google account, on a token the box mints from the account's sign-in with Google, with
//!   `suzatary_core::mail::older::gmail_q`, into `<subdir>/older-all-mail`, each message named
//!   by its Gmail id as lieer names its own, so one lieer already holds is never fetched again.
//! - A Proton account is not searched: mbsync reaches the Bridge, and offers no server search.
//!
//! Every leaf lands read and outside the `+inbox` tagging, so nothing pulled enters the triage
//! queue. The pull runs under the sync's hold, and the index is updated once, after every
//! account.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;

use suzatary_core::capability::{JunkFailure, OlderMail};
use suzatary_core::mail::account::{MailAccount, Roster, Sync};
use suzatary_core::mail::imap::{ImapError, Maildir};
use suzatary_core::mail::older::{
    self, AccountOlder, Found, IMAP_FOLDER, MAX_MATCHES, MAX_MESSAGE_BYTES, Matched, OlderQuery,
    Outcome, Plan, Range, Unsearched,
};
use suzatary_core::mail::verbs::Summary;

use crate::gmail::{self, Fetched, Lieer, MessageId};
use crate::http::Http;
use crate::imap::{Folder, Outlook};
use crate::junk::Connect;
use crate::sync::{read_sync_state, state_file, write_sync_state};
use crate::tokens::{Bearer, unix_now};

/// What a Gmail account's search covers: every label but Spam and Trash, as the API searches.
pub const GMAIL_FOLDER: &str = "All Mail";

/// The index the pulled leaves join.
pub trait Index: Send + std::marker::Sync {
    /// Index every file written since the last run.
    ///
    /// # Errors
    /// What the index said.
    fn index(&self) -> Result<(), String>;

    /// The rows the index holds for these Message-IDs, newest first; none for none.
    ///
    /// # Errors
    /// What the index said.
    fn rows(&self, ids: &[String]) -> Result<Vec<Summary>, String>;
}

/// The box's notmuch: indexed as a sync indexes it, read as `search` reads it.
pub struct Notmuched {
    syncer: crate::sync::Syncer,
    mail: crate::mailbox::Notmuch,
}

impl Notmuched {
    /// The syncer that indexes and the corpus that reads.
    #[must_use]
    pub fn new(syncer: crate::sync::Syncer, mail: crate::mailbox::Notmuch) -> Self {
        Self { syncer, mail }
    }
}

impl Index for Notmuched {
    fn index(&self) -> Result<(), String> {
        self.syncer.index().map(|_| ())
    }

    fn rows(&self, ids: &[String]) -> Result<Vec<Summary>, String> {
        rows_of(&self.mail, ids)
    }
}

/// `ids`' rows as `search` answers them; none for none, never the whole corpus.
fn rows_of(mail: &crate::mailbox::Notmuch, ids: &[String]) -> Result<Vec<Summary>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    mail.summaries(
        &crate::mailbox::by_id_query(ids.iter().map(String::as_str)),
        None,
    )
    .map_err(|failed| failed.to_string())
}

/// Mints an access token for a Google account's Gmail API from the grant the box keeps.
pub trait GoogleTokens {
    /// A token for `account`, or why there is none.
    ///
    /// # Errors
    /// The grant is missing or refused, or Google did not answer.
    fn token(
        &self,
        account: &suzatary_core::mail::address::EmailAddress,
    ) -> Result<suzatary_core::capability::Secret, Unsearched>;
}

impl GoogleTokens for crate::google::Grants {
    fn token(
        &self,
        account: &suzatary_core::mail::address::EmailAddress,
    ) -> Result<suzatary_core::capability::Secret, Unsearched> {
        crate::google::Grants::token(self, account)
            .map(|minted| minted.token)
            .map_err(|refused| match refused {
                crate::google::NotReady::NeedsLogin(needs) => {
                    Unsearched::NeedsLogin(needs.to_string())
                }
                other
                @ (crate::google::NotReady::NoClient | crate::google::NotReady::Failed(_)) => {
                    failed(other)
                }
            })
    }
}

/// Every account's server, its leaves under `mail_root`, its states under `state_dir`.
pub struct Servers<C, H, I> {
    roster: Roster,
    mail_root: PathBuf,
    state_dir: PathBuf,
    connect: C,
    http: H,
    index: I,
    google: std::sync::Arc<dyn GoogleTokens + Send + std::marker::Sync>,
}

impl<C, H, I> std::fmt::Debug for Servers<C, H, I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Servers")
            .field("mail_root", &self.mail_root)
            .finish_non_exhaustive()
    }
}

/// What one account's pull wrote, before the index is asked for its rows.
struct Landed {
    folders: Vec<String>,
    matched: Matched,
    already: usize,
    ids: Vec<String>,
    unnamed: usize,
    too_large: usize,
    beyond: usize,
    leaf: String,
}

impl Landed {
    fn count(&self) -> usize {
        self.ids.len() + self.unnamed
    }
}

/// A leaf that notes each written message's Message-ID, which the index names it by.
struct Noting<'a> {
    folder: &'a Folder,
    ids: RefCell<Vec<String>>,
    unnamed: RefCell<usize>,
}

impl<'a> Noting<'a> {
    fn new(folder: &'a Folder) -> Self {
        Self {
            folder,
            ids: RefCell::new(Vec::new()),
            unnamed: RefCell::new(0),
        }
    }
}

impl Maildir for Noting<'_> {
    fn write(&self, name: &str, unread: bool, raw: &[u8]) -> Result<(), ImapError> {
        self.folder.write(name, unread, raw)?;
        match mail_parser::MessageParser::default()
            .parse_headers(raw)
            .as_ref()
            .and_then(mail_parser::Message::message_id)
        {
            Some(id) => self.ids.borrow_mut().push(id.to_owned()),
            None => *self.unnamed.borrow_mut() += 1,
        }
        Ok(())
    }

    fn file_names(&self) -> Result<Vec<String>, ImapError> {
        self.folder.file_names()
    }
}

fn failed(why: impl std::fmt::Display) -> Unsearched {
    Unsearched::Failed(why.to_string())
}

fn imap_failed(failure: ImapError) -> Unsearched {
    match failure {
        ImapError::NeedsLogin(needs) => Unsearched::NeedsLogin(needs.to_string()),
        other @ (ImapError::Server { .. }
        | ImapError::NoUidValidity(_)
        | ImapError::Maildir { .. }
        | ImapError::State { .. }
        | ImapError::Unauthorised(_)) => failed(other),
    }
}

impl<C: Connect, H: Http, I: Index> Servers<C, H, I> {
    /// The roster's servers, `connect` opening a Microsoft account's IMAP session and `http`
    /// reaching the Gmail API, and `google` minting a Google account's token for it.
    #[must_use]
    pub fn new(
        roster: Roster,
        (mail_root, state_dir): (impl Into<PathBuf>, impl Into<PathBuf>),
        connect: C,
        http: H,
        index: I,
        google: std::sync::Arc<dyn GoogleTokens + Send + std::marker::Sync>,
    ) -> Self {
        Self {
            roster,
            mail_root: mail_root.into(),
            state_dir: state_dir.into(),
            connect,
            http,
            index,
            google,
        }
    }

    fn one(&self, email: &str, query: &OlderQuery, range: Range) -> Result<Landed, Unsearched> {
        let account = self
            .roster
            .named(email)
            .ok_or_else(|| failed(format!("{email} is not on this deployment's roster")))?;
        match &account.sync {
            Sync::MsImap(_) => self.imap(account, query, range),
            Sync::Gmail => {
                let grant = Lieer::read(&self.mail_root.join(&account.subdir)).map_err(failed)?;
                let token = Bearer::new(grant)
                    .token(&self.http, unix_now())
                    .map_err(failed)?;
                self.gmail(account, &token, query, range)
            }
            Sync::Google => {
                let token = self.google.token(account.email.address())?;
                self.gmail(account, &token, query, range)
            }
            Sync::Proton(_) => Err(Unsearched::NoServerSearch(account.email.to_string())),
        }
    }

    fn leaf(&self, account: &MailAccount, folder: &str) -> Result<(Folder, String), Unsearched> {
        let name = older::leaf(folder);
        let leaf = Folder::new(self.mail_root.join(&account.subdir).join(&name));
        leaf.ensure().map_err(failed)?;
        Ok((leaf, format!("{}/{name}", account.subdir)))
    }

    fn imap(
        &self,
        account: &MailAccount,
        query: &OlderQuery,
        range: Range,
    ) -> Result<Landed, Unsearched> {
        let criteria = older::imap_criteria(query, range)?;
        let session = self
            .connect
            .open(account)
            .map_err(|failure| match failure {
                JunkFailure::NeedsLogin(needs) => Unsearched::NeedsLogin(needs.to_string()),
                other => failed(other),
            })?;
        let mut session = Outlook::over(session);
        let (folder, leaf) = self.leaf(account, IMAP_FOLDER)?;
        let noting = Noting::new(&folder);
        let routine =
            read_sync_state(&state_file(&self.state_dir, account, None)).map_err(imap_failed)?;
        let path = state_file(&self.state_dir, account, Some(&older::leaf(IMAP_FOLDER)));
        let state = read_sync_state(&path).map_err(imap_failed)?;
        let done = older::pull(
            &mut session,
            &noting,
            &routine,
            state,
            IMAP_FOLDER,
            &criteria,
        )
        .map_err(imap_failed)?;
        write_sync_state(&path, &done.state).map_err(imap_failed)?;
        session.logout().map_err(imap_failed)?;
        Ok(Landed {
            folders: vec![IMAP_FOLDER.to_owned()],
            matched: Matched::Exactly(done.matched),
            already: done.already,
            ids: noting.ids.into_inner(),
            unnamed: noting.unnamed.into_inner(),
            too_large: done.too_large,
            beyond: done.beyond,
            leaf,
        })
    }

    fn gmail(
        &self,
        account: &MailAccount,
        token: &suzatary_core::capability::Secret,
        query: &OlderQuery,
        range: Range,
    ) -> Result<Landed, Unsearched> {
        let lieer = self.mail_root.join(&account.subdir);
        let http = &self.http;
        let listed = gmail::list(http, token, &older::gmail_q(query, range)).map_err(failed)?;
        let (folder, leaf) = self.leaf(account, GMAIL_FOLDER)?;
        // lieer names each message's file by its Gmail id, and so does this leaf.
        let mut held: BTreeSet<MessageId> = gmail_ids(&Folder::new(lieer.join("mail")))?;
        held.extend(gmail_ids(&folder)?);
        let Plan {
            wanted,
            already,
            beyond,
        } = older::plan(&listed.ids, &held, MAX_MATCHES);
        let noting = Noting::new(&folder);
        let mut too_large = 0;
        for id in wanted {
            match gmail::message(http, token, &id, MAX_MESSAGE_BYTES).map_err(failed)? {
                Fetched::Message(raw) => noting
                    .write(&format!("{}:2,S", id.as_str()), false, &raw)
                    .map_err(failed)?,
                Fetched::TooLarge => too_large += 1,
                Fetched::Gone => {}
            }
        }
        Ok(Landed {
            folders: vec![GMAIL_FOLDER.to_owned()],
            matched: listed.matched,
            already,
            ids: noting.ids.into_inner(),
            unnamed: noting.unnamed.into_inner(),
            too_large,
            beyond,
            leaf,
        })
    }

    /// One account's answer: its rows read back from the index, or why there are none.
    fn answered(&self, landed: Landed, indexed: &Result<(), String>) -> Outcome {
        if let Err(why) = indexed
            && landed.count() > 0
        {
            return Outcome::NotSearched(failed(format!(
                "{} messages were pulled into {} and the index could not take them: {why}",
                landed.count(),
                landed.leaf
            )));
        }
        match self.index.rows(&landed.ids) {
            Ok(pulled) => Outcome::Searched(Found {
                folders: landed.folders,
                matched: landed.matched,
                already_here: landed.already,
                pulled,
                unnamed: landed.unnamed,
                too_large: landed.too_large,
                beyond_cap: landed.beyond,
                local: format!("path:{}/**", landed.leaf),
            }),
            Err(why) => Outcome::NotSearched(failed(format!(
                "{} messages were pulled into {} and could not be read back: {why}",
                landed.count(),
                landed.leaf
            ))),
        }
    }
}

/// The Gmail ids `folder` holds files for, by lieer's naming, `<id>:2,<flags>`.
fn gmail_ids(folder: &Folder) -> Result<BTreeSet<MessageId>, Unsearched> {
    Ok(folder
        .file_names()
        .map_err(failed)?
        .iter()
        .filter_map(|name| MessageId::parse(name.split(':').next().unwrap_or_default()))
        .collect())
}

impl<C: Connect + std::marker::Sync, H: Http, I: Index> OlderMail for Servers<C, H, I> {
    fn search_older(
        &self,
        accounts: &[String],
        query: &OlderQuery,
        range: Range,
    ) -> Vec<AccountOlder> {
        // The sync's hold, so a removed account's mail deleted meanwhile is not pulled back.
        let held = crate::durable::hold(&crate::sync::lock(&self.state_dir));
        let landed: Vec<(String, Result<Landed, Unsearched>)> = accounts
            .iter()
            .map(|account| {
                let landed = match &held {
                    Ok(_) => self.one(account, query, range),
                    Err(why) => Err(failed(format!("the sync's hold could not be taken: {why}"))),
                };
                (account.clone(), landed)
            })
            .collect();
        let indexed = if landed
            .iter()
            .any(|(_, landed)| landed.as_ref().is_ok_and(|landed| landed.count() > 0))
        {
            self.index.index()
        } else {
            Ok(())
        };
        drop(held);
        landed
            .into_iter()
            .map(|(account, landed)| AccountOlder {
                account,
                outcome: match landed {
                    Ok(landed) => self.answered(landed, &indexed),
                    Err(why) => Outcome::NotSearched(why),
                },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::io::{Read, Write};
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use chrono::NaiveDate;
    use suzatary_core::mail::account::{Authority, MsImap, Proton};
    use suzatary_core::mail::imap::SyncState;

    use super::*;
    use crate::http::{Resp, Scripted as Answers};

    const ADA: &str = "ada@example.test";
    const GMAIL: &str = "gil@example.test";
    const PROTON: &str = "pat@example.test";
    const GOOGLE: &str = "gwen@example.test";

    /// The box's grants, as a test holds them: one token, minted for any account.
    struct Minted;

    impl GoogleTokens for Minted {
        fn token(
            &self,
            _: &suzatary_core::mail::address::EmailAddress,
        ) -> Result<suzatary_core::capability::Secret, Unsearched> {
            Ok(suzatary_core::capability::Secret::new("box-minted"))
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-older-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        dir
    }

    use suzatary_core::mail::account_email::AccountEmail;
    use suzatary_core::mail::folders::Folders;

    fn roster() -> Roster {
        Roster::new(vec![
            MailAccount {
                email: AccountEmail::new(ADA).expect("a lower-case address"),
                subdir: "ada".to_owned(),
                sync: Sync::MsImap(MsImap {
                    authority: Authority::Consumers,
                    inbox_folders: Folders::inbox(),
                    sent_folder: "Sent".to_owned(),
                }),
            },
            MailAccount {
                email: AccountEmail::new(GMAIL).expect("a lower-case address"),
                subdir: "gil".to_owned(),
                sync: Sync::Gmail,
            },
            MailAccount {
                email: AccountEmail::new(GOOGLE).expect("a lower-case address"),
                subdir: "gwen".to_owned(),
                sync: Sync::Google,
            },
            MailAccount {
                email: AccountEmail::new(PROTON).expect("a lower-case address"),
                subdir: "pat".to_owned(),
                sync: Sync::Proton(Proton {
                    channel: "pat".to_owned(),
                    smtp_user: PROTON.to_owned(),
                    send_addresses: vec![PROTON.to_owned()],
                    inbox_folders: Folders::inbox(),
                }),
            },
        ])
        .expect("a roster")
    }

    fn mail(id: &str, subject: &str, date: &str) -> String {
        format!(
            "Message-ID: <{id}@example.test>\r\nFrom: Teams <teams@example.test>\r\nTo: {ADA}\r\n\
             Subject: {subject}\r\nDate: {date}\r\nContent-Type: text/plain\r\n\r\n\
             You are invited to the team.\r\n"
        )
    }

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a date")
    }

    fn range() -> Range {
        Range::older(day("2023-01-01"), None, day("2026-10-10")).expect("a range")
    }

    fn query(text: &str) -> OlderQuery {
        text.parse().expect("a query")
    }

    // --- a scripted IMAP server under a real `imap::Client` -------------------------

    /// An IMAP server holding `messages`, whose `UID SEARCH` answers `matching` and logs what
    /// it was asked; any command it does not know is `BAD`.
    struct Imap {
        log: Arc<Mutex<Vec<String>>>,
        messages: BTreeMap<u32, String>,
        matching: Vec<u32>,
        written: Vec<u8>,
        answer: VecDeque<u8>,
    }

    impl Imap {
        fn say(&mut self, text: &str) {
            self.answer.extend(text.as_bytes());
        }

        fn command(&mut self, line: &str) {
            let (tag, command) = line.split_once(' ').expect("a tagged command");
            self.log.lock().expect("the log").push(command.to_owned());
            let ok = format!("{tag} OK done\r\n");
            if command.starts_with("LOGIN ") {
                self.say(&ok);
            } else if command.starts_with("EXAMINE ") {
                let exists = self.messages.len();
                self.say(&format!(
                    "* {exists} EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n{tag} OK [READ-ONLY] done\r\n"
                ));
            } else if command.starts_with("UID SEARCH ") {
                let uids: Vec<String> = self.matching.iter().map(u32::to_string).collect();
                self.say(&format!("* SEARCH {}\r\n{ok}", uids.join(" ")));
            } else if let Some(rest) = command.strip_prefix("UID FETCH ") {
                let (uid, what) = rest.split_once(' ').expect("a uid and a query");
                assert_eq!(what, "(BODY.PEEK[])", "every fetch peeks");
                let uid: u32 = uid.parse().expect("one uid");
                if let Some(raw) = self.messages.get(&uid).cloned() {
                    self.say(&format!(
                        "* 1 FETCH (UID {uid} BODY[] {{{}}}\r\n{raw})\r\n",
                        raw.len()
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

    impl Write for Imap {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buf);
            while let Some(end) = self.written.windows(2).position(|w| w == b"\r\n") {
                let line: Vec<u8> = self.written.drain(..end + 2).collect();
                self.command(String::from_utf8(line).expect("text").trim_end());
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Read for Imap {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = buf.len().min(self.answer.len());
            for (slot, byte) in buf.iter_mut().zip(self.answer.drain(..n)) {
                *slot = byte;
            }
            Ok(n)
        }
    }

    /// Opens a fresh [`Imap`] per account, sharing its log with the test.
    struct Fake {
        log: Arc<Mutex<Vec<String>>>,
        messages: BTreeMap<u32, String>,
        matching: Vec<u32>,
    }

    impl Connect for Fake {
        type Stream = Imap;

        fn open(&self, _account: &MailAccount) -> Result<imap::Session<Imap>, JunkFailure> {
            let server = Imap {
                log: self.log.clone(),
                messages: self.messages.clone(),
                matching: self.matching.clone(),
                written: Vec::new(),
                answer: b"* OK scripted IMAP ready\r\n".iter().copied().collect(),
            };
            let mut client = imap::Client::new(server);
            client.read_greeting().expect("a greeting");
            Ok(client
                .login("fake", "fake")
                .map_err(|(e, _)| e)
                .expect("a login"))
        }
    }

    /// The index as a log of its runs, answering a row per Message-ID it is asked for.
    #[derive(Default)]
    struct Logged {
        runs: Mutex<usize>,
    }

    impl Index for Logged {
        fn index(&self) -> Result<(), String> {
            *self.runs.lock().expect("the runs") += 1;
            Ok(())
        }

        fn rows(&self, ids: &[String]) -> Result<Vec<Summary>, String> {
            Ok(ids
                .iter()
                .map(|id| Summary {
                    hash: String::new(),
                    id: id.clone(),
                    sender: String::new(),
                    title: String::new(),
                    timestamp: None,
                    container: String::new(),
                    sources: 1,
                })
                .collect())
        }
    }

    fn servers<I: Index>(
        root: &Path,
        imap: Fake,
        http: Answers,
        index: I,
    ) -> Servers<Fake, Answers, I> {
        Servers::new(
            roster(),
            (root.join("Mail"), root.join("state")),
            imap,
            http,
            index,
            Arc::new(Minted),
        )
    }

    fn found(outcome: &Outcome) -> &Found {
        match outcome {
            Outcome::Searched(found) => found,
            Outcome::NotSearched(why) => panic!("not searched: {why}"),
        }
    }

    fn no_http() -> Answers {
        Answers::new(|req| panic!("no request: {req:?}"))
    }

    /// The scene: two invites from 2023 and 2024 that the box never fetched, and one recent
    /// message the routine pull already holds.
    fn invites(log: &Arc<Mutex<Vec<String>>>) -> Fake {
        Fake {
            log: log.clone(),
            messages: BTreeMap::from([
                (
                    1,
                    mail(
                        "invite-2023",
                        "Team invite",
                        "Mon, 06 Mar 2023 10:00:00 +0000",
                    ),
                ),
                (
                    2,
                    mail(
                        "invite-2024",
                        "Team invite",
                        "Tue, 09 Apr 2024 10:00:00 +0000",
                    ),
                ),
                (
                    3,
                    mail("recent", "Team invite", "Thu, 08 Oct 2026 10:00:00 +0000"),
                ),
            ]),
            matching: vec![1, 2, 3],
        }
    }

    fn routine_holds(root: &Path, uids: &[u32]) {
        let account = roster().named(ADA).cloned().expect("ada");
        write_sync_state(
            &state_file(&root.join("state"), &account, None),
            &SyncState {
                uidvalidity: Some("7".to_owned()),
                synced: uids.iter().copied().collect(),
                seen_pushed: BTreeSet::new(),
            },
        )
        .expect("a routine state");
    }

    /// An IMAP account: the server is examined and searched with the translated criteria,
    /// what the routine pull holds is skipped, the rest lands read in `older-inbox`, the index
    /// runs once, and each pulled message is answered by its Message-ID.
    #[test]
    fn an_imap_search_pulls_what_the_box_never_fetched() {
        let root = scratch("imap");
        routine_holds(&root, &[3]);
        let log = Arc::new(Mutex::new(Vec::new()));
        let index = Logged::default();
        let older = servers(&root, invites(&log), no_http(), index);
        let answer = older.search_older(&[ADA.to_owned()], &query("subject:invite"), range());
        let found = found(&answer[0].outcome);
        assert_eq!(found.folders, ["INBOX"]);
        assert_eq!(found.matched, Matched::Exactly(3));
        assert_eq!(found.already_here, 1);
        let ids: Vec<&str> = found.pulled.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(
            ids,
            ["invite-2024@example.test", "invite-2023@example.test"]
        );
        assert_eq!(found.local, "path:ada/older-inbox/**");
        assert_eq!(*older.index.runs.lock().expect("runs"), 1);

        let commands = log.lock().expect("the log").clone();
        assert!(
            commands.contains(&"EXAMINE \"INBOX\"".to_owned()),
            "{commands:?}"
        );
        assert!(
            commands.contains(
                &"UID SEARCH SINCE 01-Jan-2023 BEFORE 10-Oct-2026 SUBJECT \"invite\"".to_owned()
            ),
            "{commands:?}"
        );
        for forbidden in ["SELECT", "STORE", "COPY", "MOVE", "EXPUNGE", "APPEND"] {
            assert!(
                commands
                    .iter()
                    .all(|command| !command.starts_with(forbidden)),
                "{forbidden} in {commands:?}"
            );
        }
        let cur: Vec<String> = std::fs::read_dir(root.join("Mail/ada/older-inbox/cur"))
            .expect("the leaf")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(cur.len(), 2);
        assert!(
            cur.iter().all(|name| name.ends_with(":2,S")),
            "lands read: {cur:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A second search over the same range fetches nothing and runs no index.
    #[test]
    fn a_second_search_fetches_nothing_again() {
        let root = scratch("again");
        routine_holds(&root, &[3]);
        let log = Arc::new(Mutex::new(Vec::new()));
        let older = servers(&root, invites(&log), no_http(), Logged::default());
        older.search_older(&[ADA.to_owned()], &query("invite"), range());
        log.lock().expect("the log").clear();
        let again = older.search_older(&[ADA.to_owned()], &query("invite"), range());
        let found = found(&again[0].outcome);
        assert_eq!((found.already_here, found.pulled.len()), (3, 0));
        assert!(
            log.lock()
                .expect("the log")
                .iter()
                .all(|command| !command.starts_with("UID FETCH")),
            "nothing fetched twice"
        );
        assert_eq!(
            *older.index.runs.lock().expect("runs"),
            1,
            "no second index"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The cap, end to end: thirty matches, twenty-five pulled, five counted on the server.
    #[test]
    fn an_imap_search_pulls_at_most_the_cap() {
        let root = scratch("cap");
        let log = Arc::new(Mutex::new(Vec::new()));
        let messages: BTreeMap<u32, String> = (1..=30)
            .map(|uid| {
                (
                    uid,
                    mail(
                        &format!("m{uid}"),
                        "Team",
                        "Mon, 06 Mar 2023 10:00:00 +0000",
                    ),
                )
            })
            .collect();
        let fake = Fake {
            log,
            matching: messages.keys().copied().collect(),
            messages,
        };
        let older = servers(&root, fake, no_http(), Logged::default());
        let answer = older.search_older(&[ADA.to_owned()], &query("team"), range());
        let found = found(&answer[0].outcome);
        assert_eq!((found.pulled.len(), found.beyond_cap), (MAX_MATCHES, 5));
        let said = older::coverage(range(), &answer);
        assert!(
            said.contains("5 more left on the server past the cap of 25"),
            "{said}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A non-ASCII term is not put to an IMAP server, and the answer says so; a Proton account
    /// is not searched, and the answer says why. Neither opens a session.
    #[test]
    fn what_cannot_be_asked_is_not_searched_and_says_why() {
        let root = scratch("unsearched");
        let log = Arc::new(Mutex::new(Vec::new()));
        let older = servers(&root, invites(&log), no_http(), Logged::default());
        let answer = older.search_older(
            &[ADA.to_owned(), PROTON.to_owned()],
            &query("subject:équipe"),
            range(),
        );
        assert!(matches!(
            &answer[0].outcome,
            Outcome::NotSearched(Unsearched::Untranslatable(_))
        ));
        assert!(matches!(
            &answer[1].outcome,
            Outcome::NotSearched(Unsearched::NoServerSearch(_))
        ));
        assert!(log.lock().expect("the log").is_empty(), "no session opened");
        let said = older::coverage(range(), &answer);
        assert!(said.contains(&format!("{ADA}: not searched")), "{said}");
        assert!(said.contains(&format!("{PROTON}: not searched")), "{said}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two searches at once, as two rounds would ask them: the sync's hold puts one after the
    /// other, so each message is fetched once and lands once.
    #[test]
    fn two_searches_at_once_fetch_each_message_once() {
        let root = scratch("twice");
        routine_holds(&root, &[3]);
        let log = Arc::new(Mutex::new(Vec::new()));
        let older = servers(&root, invites(&log), no_http(), Logged::default());
        let answers: Vec<Vec<AccountOlder>> = std::thread::scope(|scope| {
            let asks: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| older.search_older(&[ADA.to_owned()], &query("invite"), range()))
                })
                .collect();
            asks.into_iter()
                .map(|ask| ask.join().expect("a search"))
                .collect()
        });
        let fetched = log
            .lock()
            .expect("the log")
            .iter()
            .filter(|command| command.starts_with("UID FETCH"))
            .count();
        assert_eq!(fetched, 2, "each of the two invites once");
        let mut pulled: Vec<usize> = answers
            .iter()
            .map(|answer| found(&answer[0].outcome).pulled.len())
            .collect();
        pulled.sort_unstable();
        assert_eq!(
            pulled,
            [0, 2],
            "one search pulled them, the other found them here"
        );
        assert_eq!(
            std::fs::read_dir(root.join("Mail/ada/older-inbox/cur"))
                .expect("the leaf")
                .count(),
            2
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A server that cannot be reached.
    struct Down;

    impl Connect for Down {
        type Stream = Imap;

        fn open(&self, account: &MailAccount) -> Result<imap::Session<Imap>, JunkFailure> {
            Err(JunkFailure::Unreachable {
                account: account.email.to_string(),
                why: "connecting to the server: network is unreachable".to_owned(),
            })
        }
    }

    /// Offline: neither server answers, so neither account is searched, each says why, nothing
    /// lands and no index runs.
    #[test]
    fn offline_nothing_is_searched_and_each_account_says_why() {
        let root = scratch("offline");
        lieer(&root);
        let older = Servers::new(
            roster(),
            (root.join("Mail"), root.join("state")),
            Down,
            Answers::always(Err(crate::http::HttpError::NotSent(
                "network is unreachable".to_owned(),
            ))),
            Logged::default(),
            Arc::new(Minted),
        );
        let answer = older.search_older(
            &[ADA.to_owned(), GMAIL.to_owned()],
            &query("invite"),
            range(),
        );
        for one in &answer {
            let Outcome::NotSearched(Unsearched::Failed(why)) = &one.outcome else {
                panic!("{} was searched: {:?}", one.account, one.outcome);
            };
            assert!(why.contains("unreachable"), "{why}");
        }
        assert_eq!(*older.index.runs.lock().expect("runs"), 0);
        let said = older::coverage(range(), &answer);
        assert!(said.contains(&format!("{ADA}: not searched")), "{said}");
        assert!(said.contains(&format!("{GMAIL}: not searched")), "{said}");
        let _ = std::fs::remove_dir_all(&root);
    }

    // --- a scripted Gmail API -----------------------------------------------------

    fn lieer(root: &Path) {
        let dir = root.join("Mail/gil");
        std::fs::create_dir_all(dir.join("mail/cur")).expect("lieer's maildir");
        std::fs::write(
            dir.join(".credentials.gmailieer.json"),
            r#"{"client_id":"id","client_secret":"s","refresh_token":"r","token_uri":"https://oauth.example.test/token"}"#,
        )
        .expect("lieer's grant");
        // lieer already holds a1.
        std::fs::write(
            dir.join("mail/cur/a1:2,S"),
            mail("held", "x", "Mon, 06 Mar 2023 10:00:00 +0000"),
        )
        .expect("a lieer file");
    }

    fn gmail_api(raw: &str) -> Answers {
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE, raw);
        let size = raw.len();
        Answers::new(move |req| {
            let url = req.url.as_str();
            Ok(if url.ends_with("/token") {
                Resp::new(200, r#"{"access_token":"tok","expires_in":3600}"#)
            } else if url.contains("/messages?") {
                Resp::new(200, r#"{"messages":[{"id":"b2"},{"id":"a1"}]}"#)
            } else if url.ends_with("/b2?format=minimal") {
                Resp::new(200, format!(r#"{{"sizeEstimate":{size}}}"#))
            } else if url.ends_with("/b2?format=raw") {
                Resp::new(200, format!(r#"{{"raw":"{encoded}"}}"#))
            } else {
                Resp::new(404, "not scripted")
            })
        })
    }

    /// A Gmail account: the API is asked with the translated `q`, the message lieer already
    /// holds is skipped, and the other lands read under its Gmail id.
    #[test]
    fn a_gmail_search_pulls_what_lieer_does_not_hold() {
        let root = scratch("gmail");
        lieer(&root);
        let log = Arc::new(Mutex::new(Vec::new()));
        let raw = mail(
            "invite-2023",
            "Team invite",
            "Mon, 06 Mar 2023 10:00:00 +0000",
        );
        let older = servers(&root, invites(&log), gmail_api(&raw), Logged::default());
        let answer = older.search_older(
            &[GMAIL.to_owned()],
            &query("subject:\"Team invite\""),
            range(),
        );
        let found = found(&answer[0].outcome);
        assert_eq!(found.folders, [GMAIL_FOLDER]);
        assert_eq!(
            (found.matched, found.already_here),
            (Matched::Exactly(2), 1)
        );
        assert_eq!(found.pulled[0].id, "invite-2023@example.test");
        assert_eq!(
            std::fs::read_to_string(root.join("Mail/gil/older-all-mail/cur/b2:2,S"))
                .expect("the pulled file"),
            raw
        );
        let sent: Vec<String> = older.http.sent().into_iter().map(|req| req.url).collect();
        assert!(
            sent.iter().any(|url| url.contains(
                "q=subject%3A%22Team%20invite%22%20after%3A2023%2F01%2F01%20before%3A2026%2F10%2F10"
            )),
            "{sent:?}"
        );
        assert!(sent.iter().all(|url| !url.contains("/a1")), "{sent:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A Google account is searched with the token the box mints from its own grant: no token
    /// endpoint is asked, and lieer's file, which holds no grant, is never read.
    #[test]
    fn a_google_account_is_searched_on_the_boxs_own_token() {
        let root = scratch("google");
        std::fs::create_dir_all(root.join("Mail/gwen/mail/cur")).expect("lieer's maildir");
        let log = Arc::new(Mutex::new(Vec::new()));
        let raw = mail(
            "invite-2024",
            "Team invite",
            "Mon, 04 Mar 2024 10:00:00 +0000",
        );
        let older = servers(&root, invites(&log), gmail_api(&raw), Logged::default());
        let answer = older.search_older(&[GOOGLE.to_owned()], &query("invite"), range());
        let found = found(&answer[0].outcome);
        assert_eq!(found.folders, [GMAIL_FOLDER]);
        assert!(root.join("Mail/gwen/older-all-mail/cur/b2:2,S").exists());
        let sent = older.http.sent();
        assert!(!sent.is_empty());
        assert!(
            sent.iter().all(|req| !req.url.ends_with("/token")),
            "{sent:?}"
        );
        assert!(
            sent.iter()
                .all(|req| req.header_value("Authorization") == Some("Bearer box-minted")),
            "{sent:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // --- the real index, where notmuch is on PATH ----------------------------------

    /// notmuch with its own config, never the box's.
    struct Real {
        config: PathBuf,
        mail: crate::mailbox::Notmuch,
    }

    impl Index for Real {
        fn index(&self) -> Result<(), String> {
            crate::run::succeed(
                &crate::run::Cmd::new("notmuch")
                    .arg("new")
                    .env("NOTMUCH_CONFIG", &self.config),
            )
            .map(|_| ())
            .map_err(|failed| failed.to_string())
        }

        fn rows(&self, ids: &[String]) -> Result<Vec<Summary>, String> {
            rows_of(&self.mail, ids)
        }
    }

    fn ledger(root: &Path) -> crate::state::Ledger {
        crate::state::Ledger::new(
            root.join("state.json"),
            root.join("local/mail.json"),
            || {
                Ok(suzatary_core::mail::dedup_key::DedupKey::of(
                    &suzatary_core::capability::Secret::new("12".repeat(32)),
                )
                .expect("a key"))
            },
        )
    }

    /// With the real notmuch: a message older than the window is found on the server, pulled,
    /// indexed and shown by the id the answer gave; and when the same message later lands in
    /// the routine sync's Inbox too, the index still holds it once.
    #[test]
    fn a_pulled_message_is_indexed_once_and_shown() {
        if crate::run::run(&crate::run::Cmd::new("notmuch").arg("--version")).is_err() {
            eprintln!("notmuch is not on PATH: the real-index half of the older search is not run");
            return;
        }
        let root = scratch("notmuch");
        let mail_root = root.join("Mail");
        std::fs::create_dir_all(&mail_root).expect("the mail root");
        let config = root.join("notmuch-config");
        std::fs::write(
            &config,
            format!(
                "[database]\npath={}\n[user]\nname=Ada\nprimary_email={ADA}\n[new]\ntags=new\n\
                 [maildir]\nsynchronize_flags=true\n",
                mail_root.display()
            ),
        )
        .expect("a notmuch config");
        let corpus =
            crate::mailbox::Notmuch::new(&mail_root, ledger(&root), roster()).with_config(&config);
        let log = Arc::new(Mutex::new(Vec::new()));
        let older = servers(
            &root,
            invites(&log),
            no_http(),
            Real {
                config: config.clone(),
                mail: corpus,
            },
        );
        let answer = older.search_older(&[ADA.to_owned()], &query("subject:invite"), range());
        let found = found(&answer[0].outcome);
        let first = found
            .pulled
            .iter()
            .find(|row| row.id == "invite-2023@example.test")
            .expect("the 2023 invite, as the index reads it");
        assert_eq!(first.title, "Team invite");

        let shown = older
            .index
            .mail
            .full_message(&first.id, &ledger(&root).read().expect("the ledger"))
            .expect("a read");
        let suzatary_core::mail::verbs::Show::Shown(shown) = shown else {
            panic!("show finds the pulled message, and answered {shown:?}");
        };
        assert!(shown.body.contains("You are invited"), "{}", shown.body);

        // The routine sync lands the same message in Inbox later: one message, two files.
        let inbox = Folder::new(mail_root.join("ada/Inbox"));
        inbox.ensure().expect("the inbox leaf");
        inbox
            .write(
                "copy.ms",
                true,
                mail(
                    "invite-2023",
                    "Team invite",
                    "Mon, 06 Mar 2023 10:00:00 +0000",
                )
                .as_bytes(),
            )
            .expect("a copy");
        older.index.index().expect("indexed again");
        let rows = older
            .index
            .rows(&["invite-2023@example.test".to_owned()])
            .expect("rows");
        assert_eq!(rows.len(), 1, "one message, however many files: {rows:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
