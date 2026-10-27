//! The account model: which kinds of account exist and how each is scoped. The
//! roster of addresses is the deployment's (`instance.toml`), so no address and
//! no I/O live here.
//!
//! - `inbox_query` is derived from the strategy's own `inbox_folders`, so an
//!   account cannot be synced into a folder the queue does not read.
//! - `query` is wider than `inbox_query`: it covers backfilled archives, which
//!   tagged `+inbox` would flood the queue with sent mail.
//! - An address absent from the roster cannot be sent from: [`Roster::sending`]
//!   answers `None` rather than guessing.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::mail::account_email::AccountEmail;
use crate::mail::folders::Folders;

/// The MSAL tenant a Microsoft account authenticates against.
///
/// A closed set because the login URL is built from it; a third spelling would
/// be a 400 from Microsoft the hour a token expired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "&'static str")]
pub enum Authority {
    /// Work / school tenants (M365).
    Organizations,
    /// Personal Microsoft accounts (Outlook.com, Hotmail).
    Consumers,
}

impl Authority {
    /// The tenant segment of the login URL.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Authority::Organizations => "organizations",
            Authority::Consumers => "consumers",
        }
    }

    /// Read one, or the reason this text is not one.
    pub fn parse(text: &str) -> Result<Self, RosterError> {
        match text {
            "organizations" => Ok(Authority::Organizations),
            "consumers" => Ok(Authority::Consumers),
            other => Err(RosterError::Authority(other.to_owned())),
        }
    }
}

impl TryFrom<String> for Authority {
    type Error = RosterError;
    fn try_from(text: String) -> Result<Self, RosterError> {
        Self::parse(&text)
    }
}

impl From<Authority> for &'static str {
    fn from(authority: Authority) -> Self {
        authority.as_str()
    }
}

impl std::fmt::Display for Authority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Microsoft 365 / personal Outlook, over IMAP + XOAUTH2
/// (`suzatary_core::mail::imap`, transported by `suzatary_io::imap`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MsImap {
    /// Which tenant to authenticate against.
    pub authority: Authority,
    /// The leaves under the account's maildir that count as triage input. The
    /// syncer pulls INBOX only, so widening this means teaching it to fetch more.
    pub inbox_folders: Folders,
    /// The server-side name of the sent folder ("Sent Items" on M365, "Sent" on
    /// personal Outlook: list the folders, do not guess), kept fresh through the
    /// read-only backfill path so "did I reply to X" has evidence.
    pub sent_folder: String,
}

/// Proton Mail, over `mbsync` against the local Proton Bridge daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proton {
    /// A group in the mbsync configuration, not a bare channel: the inbox
    /// channel plus a `Sync Pull` sent channel, so upstream cannot be mutated.
    pub channel: String,
    /// The Bridge SMTP login: the Proton account, while mail may go out from any
    /// address it holds.
    pub smtp_user: String,
    /// The addresses this account may send as; any other cannot be sent from.
    pub send_addresses: Vec<String>,
    /// Which synced leaves count as triage input. Not just `Inbox`, since
    /// Proton's rules file mail into custom folders; it must match mbsync's
    /// `Patterns` (test-pinned), or a synced folder silently never reaches the queue.
    pub inbox_folders: Folders,
}

/// How an account is pulled, matched exhaustively by the syncer and the send
/// path. Tagged on `sync`, so one strategy's fields are refused under another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "sync", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Sync {
    /// Gmail through lieer, which maps labels to notmuch tags itself, so no
    /// `+inbox` tagging and no `inbox_folders`.
    Gmail,
    /// A Google account signed in with Google from the box's card: its mail through lieer as
    /// [`Sync::Gmail`]'s is, under the grant the box keeps and hands lieer before each sync, and
    /// its calendars through Google's Calendar API (`crate::google`).
    Google,
    /// Microsoft, over IMAP + XOAUTH2.
    MsImap(MsImap),
    /// Proton, over the Bridge.
    Proton(Proton),
}

/// One account: its canonical identity, its maildir folder, and how it syncs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailAccount {
    /// The canonical address — also the notmuch `From` and the triage key.
    pub email: AccountEmail,
    /// The maildir folder under the mail root.
    pub subdir: String,
    /// The strategy.
    #[serde(flatten)]
    pub sync: Sync,
}

impl MailAccount {
    /// The notmuch fragment scoping a search to this account's maildir.
    ///
    /// Wider than [`MailAccount::inbox_query`]: backfilled archives are
    /// searchable history, not triage input.
    #[must_use]
    pub fn query(&self) -> String {
        format!("path:{}/**", self.subdir)
    }

    /// The leaves the syncer may blanket-tag `+inbox`, empty for a strategy
    /// that carries its own tags.
    #[must_use]
    pub fn inbox_folders(&self) -> &[String] {
        match &self.sync {
            Sync::Gmail | Sync::Google => &[],
            Sync::MsImap(ms) => ms.inbox_folders.as_slice(),
            Sync::Proton(proton) => proton.inbox_folders.as_slice(),
        }
    }

    /// True when the sync path leaves messages untagged, so the syncer must
    /// apply `+inbox` itself. lieer already syncs labels; IMAP and mbsync do
    /// not.
    #[must_use]
    pub fn label_from_folder(&self) -> bool {
        !matches!(self.sync, Sync::Gmail | Sync::Google)
    }

    /// What the syncer may blanket-tag `+inbox`: narrower than
    /// [`MailAccount::query`], which would put years of sent mail in the queue.
    #[must_use]
    pub fn inbox_query(&self) -> String {
        let folders = match &self.sync {
            Sync::Gmail | Sync::Google => return self.query(),
            Sync::MsImap(MsImap { inbox_folders, .. })
            | Sync::Proton(Proton { inbox_folders, .. }) => inbox_folders,
        };
        let leaves: Vec<String> = folders
            .as_slice()
            .iter()
            .map(|folder| format!("path:{}/{folder}/**", self.subdir))
            .collect();
        format!("({})", leaves.join(" or "))
    }

    /// The filesystem-safe token this account's per-account state is filed
    /// under; stored state is named by it, so it must not change.
    #[must_use]
    pub fn slug(&self) -> String {
        slug_of(self.email.as_str())
    }

    /// The Microsoft half of this account, or `None` when it is not one.
    #[must_use]
    pub fn microsoft(&self) -> Option<&MsImap> {
        match &self.sync {
            Sync::MsImap(ms) => Some(ms),
            _ => None,
        }
    }
}

/// The filesystem-safe token an address's state and maildir folder are filed under
/// (`her@example.test` is `her-at-example-test`).
#[must_use]
pub fn slug_of(email: &str) -> String {
    email.replace('@', "-at-").replace('.', "-")
}

/// A filesystem-safe token for a server folder name (`"Sent Items"` becomes
/// `"sent-items"`), used for maildir leaves and state files: runs of
/// non-alphanumerics become one dash, trimmed at both ends.
#[must_use]
pub fn folder_slug(folder: &str) -> String {
    let mut out = String::with_capacity(folder.len());
    let mut pending_dash = false;
    for ch in folder.chars() {
        let lowered = ch.to_ascii_lowercase();
        if lowered.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(lowered);
        } else {
            pending_dash = true;
        }
    }
    out
}

/// Which transport may send as an address.
///
/// Resolution is Proton's `send_addresses` first, then an account's identity:
/// a Proton identity is also a send address, and only the Bridge can send it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    /// Microsoft SMTP + XOAUTH2; the `From` must be the account's own address.
    Microsoft {
        /// The authenticated account's canonical address.
        email: String,
        /// Its maildir folder, for the token store's slug.
        subdir: String,
        /// Its tenant.
        authority: Authority,
    },
    /// The local Proton Bridge; the `From` may be any address the account holds.
    Proton {
        /// The Bridge login.
        smtp_user: String,
        /// The `From` address.
        from: String,
    },
    /// The Gmail API, borrowing lieer's stored grant (`gmail.modify` covers
    /// `messages.send`).
    Gmail {
        /// The maildir folder holding lieer's credential file.
        subdir: String,
        /// The `From` address.
        from: String,
    },
    /// The Gmail API, with an access token the box mints from the account's sign-in with
    /// Google, which it keeps itself.
    Google {
        /// The account's address, which is also the `From`.
        from: String,
    },
}

impl Sender {
    /// The address this send goes out as.
    #[must_use]
    pub fn from(&self) -> &str {
        match self {
            Sender::Microsoft { email, .. } => email,
            Sender::Proton { from, .. } | Sender::Gmail { from, .. } | Sender::Google { from } => {
                from
            }
        }
    }
}

/// A roster refused to be one.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RosterError {
    /// Two accounts claim the same canonical address, the key triage runs on.
    #[error("two accounts claim the address {0:?}")]
    DuplicateEmail(String),
    /// Two accounts claim the same maildir folder; provenance is derived from
    /// where a message lives, so it would be a coin flip.
    #[error("two accounts claim the maildir folder {0:?}")]
    DuplicateSubdir(String),
    /// A Microsoft account named no sent folder, without which "no trace in the
    /// corpus" is not evidence of absence.
    #[error("the Microsoft account {0:?} names no sent folder")]
    NoSentFolder(String),
    /// A Proton account named no address it may send as.
    #[error("the Proton account {0:?} names no send addresses")]
    NoSendAddresses(String),
    /// A tenant this program does not know.
    #[error("{0:?} is not an MSAL authority (organizations, consumers)")]
    Authority(String),
    /// An account's address that is not one.
    #[error("{0:?} is not an email address")]
    Email(String),
    /// An account's address with a capital in it.
    #[error("the account address {given:?} has a capital; write it as {folded:?}")]
    NotFolded {
        /// As written.
        given: String,
        /// As it must be written.
        folded: String,
    },
    /// An account that counts no folder as triage input.
    #[error("an account's inbox_folders names no folder")]
    NoFolders,
    /// A blank folder name.
    #[error("an account's inbox_folders names a blank folder")]
    BlankFolder,
}

/// An account as the journal names it: its place in the roster and how it syncs, never its
/// address, which a log line read every half hour for a year would otherwise repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountName {
    place: usize,
    strategy: &'static str,
}

impl std::fmt::Display for AccountName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "account {} ({})", self.place, self.strategy)
    }
}

/// Every account this deployment reads mail for, checked as a set (unique
/// identities and folders, a sent folder on every Microsoft account) by [`Roster::new`]; the
/// default roster holds no account.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<MailAccount>", into = "Vec<MailAccount>")]
pub struct Roster(Vec<MailAccount>);

impl TryFrom<Vec<MailAccount>> for Roster {
    type Error = RosterError;
    fn try_from(accounts: Vec<MailAccount>) -> Result<Self, RosterError> {
        Self::new(accounts)
    }
}

impl From<Roster> for Vec<MailAccount> {
    fn from(roster: Roster) -> Self {
        roster.0
    }
}

impl Roster {
    /// Check a list of accounts into a roster, or say why it is not one.
    pub fn new(accounts: Vec<MailAccount>) -> Result<Self, RosterError> {
        let mut emails: BTreeSet<&str> = BTreeSet::new();
        let mut subdirs: BTreeSet<&str> = BTreeSet::new();
        for account in &accounts {
            if !emails.insert(account.email.as_str()) {
                return Err(RosterError::DuplicateEmail(account.email.to_string()));
            }
            if !subdirs.insert(account.subdir.as_str()) {
                return Err(RosterError::DuplicateSubdir(account.subdir.clone()));
            }
            // Exhaustive so a new strategy must decide its invariant here.
            match &account.sync {
                Sync::MsImap(ms) if ms.sent_folder.trim().is_empty() => {
                    return Err(RosterError::NoSentFolder(account.email.to_string()));
                }
                Sync::Proton(proton) if proton.send_addresses.is_empty() => {
                    return Err(RosterError::NoSendAddresses(account.email.to_string()));
                }
                Sync::Gmail | Sync::Google | Sync::MsImap(_) | Sync::Proton(_) => {}
            }
        }
        Ok(Self(accounts))
    }

    /// Every account, in the order the instance file named them.
    #[must_use]
    pub fn accounts(&self) -> &[MailAccount] {
        &self.0
    }

    /// Every account with the name a log line gives it, in the roster's order: an account held
    /// is an account named.
    pub fn names(&self) -> impl Iterator<Item = (AccountName, &MailAccount)> {
        self.0.iter().enumerate().map(|(at, account)| {
            let strategy = match account.sync {
                Sync::Gmail => "gmail",
                Sync::Google => "google",
                Sync::MsImap(_) => "microsoft",
                Sync::Proton(_) => "proton",
            };
            (
                AccountName {
                    place: at + 1,
                    strategy,
                },
                account,
            )
        })
    }

    /// The account with this canonical address.
    ///
    /// A filter naming an absent account is a refusal, not an empty page: the
    /// two look identical and mean opposite things.
    #[must_use]
    pub fn named(&self, email: &str) -> Option<&MailAccount> {
        self.0
            .iter()
            .find(|account| account.email.as_str() == email)
    }

    /// The Microsoft account with this address, case-insensitively.
    #[must_use]
    pub fn microsoft(&self, email: &str) -> Option<&MailAccount> {
        self.microsoft_named(email).map(|(_, account)| account)
    }

    /// [`Roster::microsoft`], with the name a log line gives it.
    #[must_use]
    pub fn microsoft_named(&self, email: &str) -> Option<(AccountName, &MailAccount)> {
        self.names()
            .find(|(_, account)| account.email.is(email) && account.microsoft().is_some())
    }

    /// Which transport may send as `address`, or `None` when nothing may; the
    /// order is [`Sender`]'s.
    #[must_use]
    pub fn sending(&self, address: &str) -> Option<Sender> {
        for account in &self.0 {
            if let Sync::Proton(proton) = &account.sync
                && proton
                    .send_addresses
                    .iter()
                    .any(|held| held.eq_ignore_ascii_case(address))
            {
                return Some(Sender::Proton {
                    smtp_user: proton.smtp_user.clone(),
                    from: address.to_owned(),
                });
            }
        }
        for account in &self.0 {
            if !account.email.is(address) {
                continue;
            }
            return match &account.sync {
                Sync::MsImap(ms) => Some(Sender::Microsoft {
                    email: account.email.to_string(),
                    subdir: account.subdir.clone(),
                    authority: ms.authority,
                }),
                Sync::Gmail => Some(Sender::Gmail {
                    subdir: account.subdir.clone(),
                    from: account.email.to_string(),
                }),
                Sync::Google => Some(Sender::Google {
                    from: account.email.to_string(),
                }),
                Sync::Proton(_) => None,
            };
        }
        None
    }

    /// Every address this roster can send from, sorted, so a refusal can name
    /// the paths that exist.
    #[must_use]
    pub fn sendable(&self) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for account in &self.0 {
            match &account.sync {
                Sync::Gmail | Sync::Google | Sync::MsImap(_) => {
                    out.insert(account.email.to_string());
                }
                Sync::Proton(proton) => {
                    out.extend(proton.send_addresses.iter().cloned());
                }
            }
        }
        out.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gmail(email: &str, subdir: &str) -> MailAccount {
        MailAccount {
            email: crate::mail::account_email::AccountEmail::new(email)
                .expect("a lower-case address"),
            subdir: subdir.to_owned(),
            sync: Sync::Gmail,
        }
    }

    fn ms(email: &str, subdir: &str) -> MailAccount {
        MailAccount {
            email: crate::mail::account_email::AccountEmail::new(email)
                .expect("a lower-case address"),
            subdir: subdir.to_owned(),
            sync: Sync::MsImap(MsImap {
                authority: Authority::Organizations,
                inbox_folders: crate::mail::folders::Folders::inbox(),
                sent_folder: "Sent Items".to_owned(),
            }),
        }
    }

    fn proton(email: &str, subdir: &str) -> MailAccount {
        MailAccount {
            email: crate::mail::account_email::AccountEmail::new(email)
                .expect("a lower-case address"),
            subdir: subdir.to_owned(),
            sync: Sync::Proton(Proton {
                channel: "proton-all".to_owned(),
                smtp_user: "someone@proton.test".to_owned(),
                send_addresses: vec![email.to_owned(), "other@example.test".to_owned()],
                inbox_folders: crate::mail::folders::Folders::new(vec![
                    "Inbox".to_owned(),
                    "Folders/example".to_owned(),
                ])
                .expect("named folders"),
            }),
        }
    }

    /// A log line names an account by its place and strategy, never by its address.
    #[test]
    fn an_account_is_named_by_its_place_never_its_address() {
        let roster = Roster::new(vec![
            gmail("one@example.test", "one"),
            ms("two@example.test", "two"),
        ])
        .expect("a roster");
        let named: Vec<String> = roster.names().map(|(name, _)| name.to_string()).collect();
        assert_eq!(
            named.len(),
            roster.accounts().len(),
            "every account is named"
        );
        let (name, _) = roster
            .microsoft_named("TWO@example.test")
            .expect("held, case-insensitively");
        assert_eq!(name.to_string(), "account 2 (microsoft)");
        assert!(named.iter().all(|n| !n.contains('@')));
    }

    #[test]
    fn a_slug_is_the_address_with_its_punctuation_flattened() {
        assert_eq!(
            ms("ada@university.example", "study").slug(),
            "ada-at-university-example".to_owned()
        );
    }

    #[test]
    fn a_folder_slug_has_no_leading_or_trailing_dashes() {
        assert_eq!(folder_slug("Sent Items"), "sent-items");
        assert_eq!(folder_slug("INBOX"), "inbox");
        assert_eq!(folder_slug("robotics/worldwide"), "robotics-worldwide");
        assert_eq!(folder_slug(" [Gmail]/Sent Mail "), "gmail-sent-mail");
        assert_eq!(folder_slug("---"), "");
    }

    // --- Sending -------------------------------------------------------------

    /// Proton first: a Proton identity is also a send address, and only the
    /// Bridge can send it.
    #[test]
    fn the_send_identity_resolves_proton_before_an_account_match() {
        let roster = Roster::new(vec![
            gmail("her@gmail.test", "her-gmail"),
            ms("her@study.test", "study"),
            proton("contact@example.test", "contact"),
        ])
        .expect("a roster");

        assert_eq!(
            roster.sending("contact@example.test"),
            Some(Sender::Proton {
                smtp_user: "someone@proton.test".to_owned(),
                from: "contact@example.test".to_owned(),
            })
        );
        // An alias the Proton account holds but that is nobody's identity.
        assert_eq!(
            roster.sending("other@example.test"),
            Some(Sender::Proton {
                smtp_user: "someone@proton.test".to_owned(),
                from: "other@example.test".to_owned(),
            })
        );
        assert_eq!(
            roster.sending("her@study.test"),
            Some(Sender::Microsoft {
                email: "her@study.test".to_owned(),
                subdir: "study".to_owned(),
                authority: Authority::Organizations,
            })
        );
        assert_eq!(
            roster.sending("her@gmail.test"),
            Some(Sender::Gmail {
                subdir: "her-gmail".to_owned(),
                from: "her@gmail.test".to_owned(),
            })
        );
    }

    /// A Google account sends on the box's own token, never through lieer's file.
    #[test]
    fn a_google_account_sends_on_the_boxs_own_grant() {
        let roster = Roster::new(vec![MailAccount {
            email: AccountEmail::new("gwen@example.test").expect("a lower-case address"),
            subdir: "gwen".to_owned(),
            sync: Sync::Google,
        }])
        .expect("a roster");
        assert_eq!(
            roster.sending("gwen@example.test"),
            Some(Sender::Google {
                from: "gwen@example.test".to_owned(),
            })
        );
    }

    /// An address the roster does not hold has no send path, and the roster
    /// names the ones that exist.
    #[test]
    fn an_unknown_address_has_no_send_path_and_the_roster_says_which_do() {
        let roster = Roster::new(vec![
            gmail("her@gmail.test", "her-gmail"),
            proton("contact@example.test", "contact"),
        ])
        .expect("a roster");
        assert_eq!(roster.sending("stranger@example.test"), None);
        assert_eq!(
            roster.sendable(),
            vec![
                "contact@example.test".to_owned(),
                "her@gmail.test".to_owned(),
                "other@example.test".to_owned(),
            ]
        );
    }
}
