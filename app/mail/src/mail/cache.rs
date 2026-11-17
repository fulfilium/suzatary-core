//! The mail a sync keeps on the box: a cache of the server's, which syncing again rebuilds.

use crate::mail::account::MailAccount;
use crate::mail::grants::Revoked;
use crate::mail::removal::Removal;

/// What deleting an account's kept mail came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forgot {
    /// No mail was kept for it.
    NotKept,
    /// Its folder was deleted, and the index forgot it.
    Deleted {
        /// The folder.
        folder: String,
    },
    /// Its folder was deleted, and the index still names its messages until the next sync.
    DeletedIndexStale {
        /// The folder.
        folder: String,
        /// Why the index was not told.
        why: String,
    },
    /// Nothing, or not all of it, went.
    Failed {
        /// Why.
        why: String,
    },
}

impl Forgot {
    /// What a removal's outcome says of it.
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Forgot::NotKept => "no mail was kept for it".to_owned(),
            Forgot::Deleted { folder } => {
                format!("its mail in {folder} was deleted, and the index forgot it")
            }
            Forgot::DeletedIndexStale { folder, why } => format!(
                "its mail in {folder} was deleted, and the index forgets it at the next sync: \
                 {why}"
            ),
            Forgot::Failed { why } => format!("its mail was not deleted: {why}"),
        }
    }
}

/// What deleting what the box kept for a removed account came to, one arm per
/// [`Removal`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forgotten {
    /// Its sign-in.
    SignIn(Result<Revoked, String>),
    /// Its mail.
    Mail(Forgot),
    /// Its sign-in and its mail.
    Both(Result<Revoked, String>, Forgot),
}

impl Forgotten {
    /// Nothing of `removal` went, for `why`.
    #[must_use]
    pub fn failed(removal: Removal, why: &str) -> Self {
        let mail = || Forgot::Failed {
            why: why.to_owned(),
        };
        match removal {
            Removal::SignIn => Self::SignIn(Err(why.to_owned())),
            Removal::Mail => Self::Mail(mail()),
            Removal::Both => Self::Both(Err(why.to_owned()), mail()),
        }
    }

    /// What the conversation says of it.
    #[must_use]
    pub fn sentence(&self) -> String {
        let grants = |grants: &Result<Revoked, String>| match grants {
            Ok(revoked) => revoked.sentence().to_owned(),
            Err(failed) => format!("its sign-in is still kept: {failed}"),
        };
        match self {
            Self::SignIn(revoked) => grants(revoked),
            Self::Mail(forgot) => forgot.sentence(),
            Self::Both(revoked, forgot) => format!("{}; {}", grants(revoked), forgot.sentence()),
        }
    }
}

/// Deletes what the box kept for one removed account, as its [`Removal`] says, under the
/// sync's hold; held in no capability record, so only a mail account's removal reaches it.
pub trait Forget {
    /// Deletes `removal` of what was kept for `account`, answering what went.
    fn forget(&self, account: &MailAccount, removal: Removal) -> Forgotten;
}

/// The folder under the mail root `account`'s mail is kept in, `None` for a name that could
/// reach outside it (empty, `.`, `..`, a separator or a control character): the one name a
/// delete may be handed.
#[must_use]
pub fn folder(account: &MailAccount) -> Option<&str> {
    let name = account.subdir.as_str();
    let plain = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control);
    plain.then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::account::Sync;

    fn kept_in(subdir: &str) -> MailAccount {
        MailAccount {
            email: crate::mail::account_email::AccountEmail::new("ada@example.test")
                .expect("a lower-case address"),
            subdir: subdir.to_owned(),
            sync: Sync::Gmail,
        }
    }

    /// Only one plain folder name is ever handed to a delete.
    #[test]
    fn a_folder_that_could_reach_outside_the_mail_root_is_none() {
        assert_eq!(
            folder(&kept_in("ada-at-example-test")),
            Some("ada-at-example-test")
        );
        for bad in ["", ".", "..", "a/b", "../x", "/", "a\\b", "a\nb"] {
            assert_eq!(folder(&kept_in(bad)), None, "{bad:?}");
        }
    }
}
