//! The syncer: pull every account through its kind's puller (`crate::pullers`), index once,
//! and tag the leaves whose sync path carries no tags. The one module that writes the maildir
//! and the index, beside `crate::older`, which writes what a server search matched into leaves
//! of its own and indexes through [`Syncer::index`].
//!
//! One account's failure is an [`AccountSync`] with a failed outcome, and the run goes on.

use std::path::{Path, PathBuf};

use suzatary_core::connect::Failure;
use suzatary_core::mail::account::{MailAccount, Roster, Sync, folder_slug};
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::mail::health::Outcome;
use suzatary_core::mail::imap::{ImapError, Pull, SyncState, window_start};
use suzatary_core::mail::puller::{Puller, Unpulled};
use suzatary_core::mail::verbs::{AccountSync, Backfilled, SyncReport};

use crate::oauth::TokenStore;
use crate::present::Present as _;
use crate::pullers::{
    Google, Granted, LIEER, Lieer, MBSYNC, Mbsync, Microsoft, Open as _, pull_into,
};
use crate::run::{Cmd, succeed};

/// The file whose hold a sync and a removed account's mail delete both take, in the sync's own
/// state directory.
#[must_use]
pub fn lock(state_dir: &Path) -> PathBuf {
    state_dir.join("sync.lock")
}

/// The syncer, bound to one deployment.
pub struct Syncer {
    accounts: Roster,
    mail_root: PathBuf,
    notmuch: PathBuf,
    /// The index's config, set as `NOTMUCH_CONFIG` on every call when named, so the index a sync
    /// writes is the one the reads ask.
    config: Option<PathBuf>,
    mbsyncrc: PathBuf,
    state_dir: PathBuf,
    tokens: TokenStore,
    grants: crate::google::Grants,
}

impl std::fmt::Debug for Syncer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Syncer")
            .field("mail_root", &self.mail_root)
            .field("accounts", &self.accounts.accounts().len())
            .finish_non_exhaustive()
    }
}

impl Syncer {
    /// Build one out of what the instance file said.
    #[must_use]
    pub fn new(
        accounts: Roster,
        mail_root: impl Into<PathBuf>,
        notmuch: impl Into<PathBuf>,
        mbsyncrc: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
        (tokens, grants): (TokenStore, crate::google::Grants),
    ) -> Self {
        Self {
            accounts,
            mail_root: mail_root.into(),
            notmuch: notmuch.into(),
            config: None,
            mbsyncrc: mbsyncrc.into(),
            state_dir: state_dir.into(),
            tokens,
            grants,
        }
    }

    /// The notmuch config every index call carries as `NOTMUCH_CONFIG`.
    #[must_use]
    pub fn with_config(mut self, config: impl Into<PathBuf>) -> Self {
        self.config = Some(config.into());
        self
    }

    /// The roster this syncer pulls.
    #[must_use]
    pub fn accounts(&self) -> &Roster {
        &self.accounts
    }

    /// Pull every account, index once, tag the leaves that need it, under [`lock`]'s hold, so a
    /// removed account's mail deleted meanwhile is not pulled back.
    #[must_use]
    pub fn sync(&self, today: chrono::NaiveDate) -> SyncReport {
        // A sync that could not take the hold still syncs, and its report says so.
        let held = crate::durable::hold(&lock(&self.state_dir));
        let unheld = held
            .as_ref()
            .err()
            .map(|failed| format!("the sync's hold could not be taken: {failed}"));
        let since = window_start(today);
        let synced: Vec<AccountSync> = self
            .accounts
            .accounts()
            .iter()
            .map(|account| self.one(account, since))
            .collect();

        let indexed = said(&self.notmuch(&["new"])).unwrap_or_else(|failed| failed);
        // lieer maps Gmail's labels to tags; IMAP and mbsync leave a folder
        // untagged, and untagged mail never reaches the queue, so each account's failure to
        // tag joins the report's line beside what indexing said.
        let untagged = self
            .accounts
            .accounts()
            .iter()
            .filter(|account| account.label_from_folder())
            .filter_map(|account| {
                said(&self.notmuch(&["tag", "+inbox", &account.inbox_query()]))
                    .err()
                    .map(|failed| format!("tagging {}'s inbox: {}", account.email, tail(&failed)))
            });
        SyncReport {
            synced,
            notmuch: notmuch_line(&tail(&indexed), untagged),
            unheld,
        }
    }

    fn notmuch(&self, args: &[&str]) -> Cmd {
        crate::mailbox::indexed_by(Cmd::new(&self.notmuch).args(args), self.config.as_deref())
    }

    fn one(&self, account: &MailAccount, since: chrono::NaiveDate) -> AccountSync {
        let (outcome, tail) = match self.puller(account).and_then(|puller| puller.pull(since)) {
            Ok(said) => (Outcome::Succeeded, said),
            Err(Unpulled { class, said }) => (Outcome::Failed(class), said),
        };
        AccountSync {
            account: account.email.to_string(),
            outcome,
            tail,
        }
    }

    /// The puller for `account`'s kind.
    fn puller(&self, account: &MailAccount) -> Result<Box<dyn Puller>, Unpulled> {
        let maildir = self.mail_root.join(&account.subdir);
        let lieer = |maildir: PathBuf| Lieer {
            program: PathBuf::from(LIEER),
            maildir,
        };
        Ok(match &account.sync {
            Sync::Gmail => Box::new(lieer(maildir)),
            Sync::Google => Box::new(Google {
                account: EmailAddress::new(account.email.as_str()).map_err(|refused| Unpulled {
                    class: Failure::Unclassified,
                    said: refused.message().to_owned(),
                })?,
                lieer: lieer(maildir),
                grants: self.grants.clone(),
            }),
            Sync::Proton(proton) => Box::new(Mbsync {
                program: PathBuf::from(MBSYNC),
                config: self.mbsyncrc.clone(),
                channel: proton.channel.clone(),
            }),
            Sync::MsImap(ms) => Box::new(Microsoft {
                account: account.clone(),
                sent_folder: ms.sent_folder.clone(),
                maildir,
                state_dir: self.state_dir.clone(),
                open: Granted::new(self.tokens.clone()),
            }),
        })
    }

    /// `notmuch new`, as a sync runs it, for files written outside a sync; what it said.
    ///
    /// # Errors
    /// What notmuch said when it failed, or why it could not be run.
    pub fn index(&self) -> Result<String, String> {
        said(&self.notmuch(&["new"]))
    }

    /// A one-off widening of the corpus for one server folder, kept out of
    /// the routine sync because it is expensive.
    pub fn backfill(
        &self,
        account: &MailAccount,
        folder: &str,
        subdir: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Backfilled, ImapError> {
        let mut session = Granted::new(self.tokens.clone()).open(account)?;
        let leaf = subdir.map_or_else(|| folder_slug(folder), str::to_owned);
        let fetched = pull_into(
            &mut session,
            &self.mail_root.join(&account.subdir).join(&leaf),
            &state_file(&self.state_dir, account, Some(folder)),
            &Pull::Backfill {
                folder: folder.to_owned(),
                limit,
            },
        )?;
        session.logout()?;
        self.index().map_err(|failed| ImapError::Maildir {
            what: "indexed",
            why: format!(
                "{fetched} messages were written and notmuch could not index them: {failed}"
            ),
        })?;
        Ok(Backfilled {
            account: account.email.to_string(),
            folder: folder.to_owned(),
            fetched,
        })
    }
}

/// Where one leaf's [`SyncState`] is kept under `state_dir`: `key` is the server folder a leaf
/// other than the INBOX pull's is kept for, `None` for INBOX.
pub(crate) fn state_file(state_dir: &Path, account: &MailAccount, key: Option<&str>) -> PathBuf {
    let name = match key {
        None => format!("{}.json", account.slug()),
        Some(folder) => format!("{}-{}.json", account.slug(), folder_slug(folder)),
    };
    state_dir.join(name)
}

pub(crate) fn read_sync_state(path: &Path) -> Result<SyncState, ImapError> {
    let failed = |why: &dyn std::fmt::Display| ImapError::State {
        what: "read",
        why: format!("{}: {why}", path.display()),
    };
    // A state file that is not there is a first sync, which is a fact.
    match std::fs::read_to_string(path)
        .present()
        .map_err(|e| failed(&e))?
    {
        None => Ok(SyncState::default()),
        Some(text) => serde_json::from_str(&text).map_err(|e| failed(&e)),
    }
}

pub(crate) fn write_sync_state(path: &Path, state: &SyncState) -> Result<(), ImapError> {
    let failed = |why: String| ImapError::State { what: "write", why };
    let text = serde_json::to_string_pretty(state).map_err(|e| failed(e.to_string()))?;
    crate::durable::replace(
        path,
        (text + "\n").as_bytes(),
        crate::durable::Mode::Private,
    )
    .map_err(|e| failed(format!("{}: {e}", path.display())))
}

/// What `cmd` said on both streams when it exited zero (`gmi` reports progress on stderr), or
/// why it did not. A program that is not there is one failed account, not a crashed sweep.
pub(crate) fn said(cmd: &Cmd) -> Result<String, String> {
    succeed(cmd)
        .map(|ran| format!("{}{}", ran.out(), ran.err()))
        .map_err(|failed| failed.to_string())
}

/// The report's notmuch line: what indexing said, then each account that could not be tagged,
/// leaving out an index line that is empty so the line never opens with a separator.
fn notmuch_line(indexed: &str, untagged: impl Iterator<Item = String>) -> String {
    std::iter::once(indexed.to_owned())
        .filter(|line| !line.is_empty())
        .chain(untagged)
        .collect::<Vec<_>>()
        .join("; ")
}

/// The last line of some output, as an [`Excerpt`](suzatary_core::excerpt::Excerpt): it is kept in
/// the sync report and read by the agent, so it names no address.
pub(crate) fn tail(text: &str) -> String {
    suzatary_core::excerpt::Excerpt::of(text.trim().lines().next_back().unwrap_or(""))
        .as_str()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No state is a first sync; a state that is there and cannot be read is an error, never
    /// a first sync that would fetch the whole folder again.
    #[test]
    fn only_a_missing_sync_state_is_a_first_sync() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let path = dir.path().join("account.json");
        assert_eq!(
            read_sync_state(&path).expect("a first sync"),
            SyncState::default()
        );
        std::fs::create_dir(&path).expect("a directory where the file belongs");
        assert!(read_sync_state(&path).is_err());
    }

    #[test]
    fn a_sync_state_file_round_trips_and_an_absent_one_is_a_first_sync() {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-sync-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = dir.join("her-at-example-test.json");
        assert_eq!(
            read_sync_state(&path).expect("a first sync"),
            SyncState::default()
        );

        let state = SyncState {
            uidvalidity: Some("7".to_owned()),
            synced: [1u32, 2, 3].into_iter().collect(),
            seen_pushed: [1u32].into_iter().collect(),
        };
        write_sync_state(&path, &state).expect("writes");
        assert_eq!(read_sync_state(&path).expect("reads"), state);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
