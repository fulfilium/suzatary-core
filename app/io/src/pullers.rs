//! The four pullers, one per account kind: Microsoft's IMAP, mbsync from the Proton Bridge, and
//! lieer from Gmail on its own grant or on the box's Google grant. Each pulls one account into
//! its maildir as a `suzatary_core::mail::puller::Puller`; the syncer runs one per account and
//! indexes once after them all.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use suzatary_core::connect::Failure;
use suzatary_core::mail::account::{MailAccount, MsImap};
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::mail::declared::{Declared, Landed, declared, line};
use suzatary_core::mail::imap::{INBOX, ImapError, Pull, Pulled, Session, pull};
use suzatary_core::mail::puller::{Puller, Unpulled};

use crate::google::{Grants, NotReady};
use crate::imap::{Folder, Outlook};
use crate::oauth::TokenStore;
use crate::run::Cmd;
use crate::sync::{read_sync_state, said, state_file, tail, write_sync_state};

/// lieer's program, looked up on `PATH`.
pub const LIEER: &str = "gmi";
/// mbsync's program, looked up on `PATH`.
pub const MBSYNC: &str = "mbsync";

/// A Microsoft account: every folder it declares ([`declared`]), each pulled on its own so one
/// that fails costs that folder alone: its inbox folders as windows that land unread, then its
/// Sent folder read-only into its own leaf and landed read, so sent mail is searchable and never
/// queued. A declared folder the server does not list is not asked for, and fails the pull by
/// name.
pub struct Microsoft<O> {
    /// The account, whose address the session signs in as and whose slug names its state files.
    pub account: MailAccount,
    /// Its folders as the instance file declares them.
    pub declares: MsImap,
    /// Its maildir, `<mail root>/<subdir>`.
    pub maildir: PathBuf,
    /// Where each leaf's sync state is kept.
    pub state_dir: PathBuf,
    /// How its session is opened.
    pub open: O,
}

impl<O: Open> Puller for Microsoft<O> {
    fn pull(&self, since: chrono::NaiveDate) -> Result<String, Unpulled> {
        let mut session = self.open.open(&self.account)?;
        let listed = session.list()?;
        let landed: Vec<(Declared, Landed)> = declared(&self.declares, since)
            .into_iter()
            .map(|folder| {
                let landed = if folder.listed_in(&listed) {
                    let state = match folder.pull.folder() {
                        INBOX => None,
                        other => Some(other),
                    };
                    pull_into(
                        &mut session,
                        &self.maildir.join(&folder.leaf),
                        &state_file(&self.state_dir, &self.account, state),
                        &folder.pull,
                    )
                    .map_or_else(Landed::Failed, Landed::Pulled)
                } else {
                    Landed::Unlisted
                };
                (folder, landed)
            })
            .collect();
        let said = line(&landed);
        session.logout()?;
        said
    }
}

/// How a Microsoft account's IMAP session is opened: authenticated, nothing selected.
pub trait Open {
    /// What the session runs over.
    type Stream: Read + Write;

    /// An authenticated session for `account`.
    ///
    /// # Errors
    /// [`ImapError::NeedsLogin`] when only a sign-in mends the account, else the token's or
    /// the server's failure.
    fn open(&self, account: &MailAccount) -> Result<Outlook<Self::Stream>, ImapError>;
}

/// Microsoft's IMAP endpoint over TLS, on a token minted silently from the account's grant.
#[derive(Clone)]
pub struct Granted {
    tokens: TokenStore,
}

impl Granted {
    /// Over this store's grants.
    #[must_use]
    pub fn new(tokens: TokenStore) -> Self {
        Self { tokens }
    }
}

impl Open for Granted {
    type Stream = crate::imap::TlsStream;

    fn open(&self, account: &MailAccount) -> Result<Outlook, ImapError> {
        let token = self
            .tokens
            .access::<suzatary_core::mail::oauth::Imap>(account)
            .map_err(|failed| ImapError::Unauthorised(failed.to_string()))?
            .required()
            .map_err(ImapError::NeedsLogin)?;
        Outlook::connect(
            crate::imap::IMAP_HOST,
            crate::imap::IMAP_PORT,
            account.email.as_str(),
            &token,
        )
    }
}

/// One [`Pull`] into the maildir leaf `leaf`, its state kept at `state`, so a backfill and the
/// INBOX pull never share UID sets; what it did.
pub(crate) fn pull_into<S: Session + ?Sized>(
    session: &mut S,
    leaf: &Path,
    state: &Path,
    plan: &Pull,
) -> Result<Pulled, ImapError> {
    let folder = Folder::new(leaf);
    folder.ensure()?;
    let done = pull(session, &folder, read_sync_state(state)?, plan)?;
    write_sync_state(state, &done.state)?;
    Ok(done)
}

/// A Gmail account through lieer (`gmi sync` in its maildir), which reads lieer's own grant
/// and maps Gmail's labels to tags itself.
pub struct Lieer {
    /// lieer's program, [`LIEER`] on the box.
    pub program: PathBuf,
    /// The account's maildir, lieer's repository.
    pub maildir: PathBuf,
}

impl Puller for Lieer {
    fn pull(&self, _since: chrono::NaiveDate) -> Result<String, Unpulled> {
        ran(&Cmd::new(&self.program)
            .arg("sync")
            .cwd(&self.maildir)
            .pass(crate::run::NETWORK))
    }
}

/// A Google account signed in from the box's card: lieer, handed an access token minted from
/// the box's grant (never the grant) before it syncs, its repository begun without lieer's own
/// sign-in where none is.
pub struct Google {
    /// lieer, in the account's maildir.
    pub lieer: Lieer,
    /// The account.
    pub account: EmailAddress,
    /// The grants the token is minted from.
    pub grants: Grants,
}

impl Puller for Google {
    fn pull(&self, since: chrono::NaiveDate) -> Result<String, Unpulled> {
        let not_ready = |failed: NotReady| Unpulled {
            class: failed.class(),
            said: failed.to_string(),
        };
        let Lieer { program, maildir } = &self.lieer;
        self.grants
            .ready(&self.account, maildir)
            .map_err(not_ready)?;
        if !maildir.join(".gmailieer.json").exists() {
            said(
                &Cmd::new(program)
                    .args(["init", "--no-auth", self.account.as_str()])
                    .cwd(maildir)
                    .pass(crate::run::NETWORK),
            )
            .map_err(|failed| not_ready(NotReady::Failed(tail(&failed))))?;
        }
        self.lieer.pull(since)
    }
}

/// A Proton account through mbsync from the Bridge, pinned to the deployment's configuration
/// with `-c`, since which file it reads is a security property.
pub struct Mbsync {
    /// mbsync's program, [`MBSYNC`] on the box.
    pub program: PathBuf,
    /// The deployment's `mbsyncrc`.
    pub config: PathBuf,
    /// The account's channel in it.
    pub channel: String,
}

impl Puller for Mbsync {
    fn pull(&self, _since: chrono::NaiveDate) -> Result<String, Unpulled> {
        // mbsync's `PassCmd` asks the secret store, so it is given what the store reads.
        ran(&Cmd::new(&self.program)
            .arg("-c")
            .arg(&self.config)
            .arg(&self.channel)
            .pass(crate::secrets::STORE))
    }
}

/// A syncing program's last line when it exited zero, or its failure's last line.
fn ran(cmd: &Cmd) -> Result<String, Unpulled> {
    said(cmd)
        .map(|output| tail(&output))
        .map_err(|failed| Unpulled {
            class: Failure::Unclassified,
            said: tail(&failed),
        })
}
