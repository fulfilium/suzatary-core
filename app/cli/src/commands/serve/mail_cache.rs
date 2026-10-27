//! What this box kept for a removed account, deleted under the sync's hold: its sign-in, which
//! goes with any account that holds one, and the mail its sync kept, on the card's wider yes.

use std::path::PathBuf;

use suzatary_core::mail::account::MailAccount;
use suzatary_core::mail::cache::{Forgot, Forgotten};
use suzatary_core::mail::removal::Removal;
use suzatary_io::present::Present as _;
use suzatary_io::run::{Cmd, succeed};

/// How long telling the index may take before the delete answers that it will at the next sync.
const REINDEX: std::time::Duration = std::time::Duration::from_secs(120);

/// Each account's mail under the mail root, one folder per account, and the index over it.
pub struct MailCache {
    root: PathBuf,
    notmuch: PathBuf,
    config: Option<PathBuf>,
    /// The sync's hold, taken for the delete and the index, so a sync begun before the tap
    /// cannot pull the account's mail back after it, nor refresh a sign-in being revoked.
    lock: PathBuf,
    /// Where the accounts' sign-ins are kept.
    tokens: suzatary_io::oauth::TokenStore,
}

impl MailCache {
    /// The mail kept under `root`, indexed by the `notmuch` binary under `config`, or under the
    /// config its environment names, as the sync's, and held against the sync by `lock`.
    #[must_use]
    pub fn new(
        root: PathBuf,
        notmuch: PathBuf,
        config: Option<PathBuf>,
        lock: PathBuf,
        tokens: suzatary_io::oauth::TokenStore,
    ) -> Self {
        Self {
            root,
            notmuch,
            config,
            lock,
            tokens,
        }
    }

    /// The mail `instance` keeps, indexed as its mailbox is and held as its sync holds.
    #[must_use]
    pub fn of(instance: &crate::instance::Instance) -> Self {
        crate::wiring::Ground::of(instance).mail_cache()
    }

    /// The index told what the maildir no longer holds, so no reading asks it for a file gone;
    /// no hook runs, since a hook may sync.
    fn reindex(&self) -> Result<(), String> {
        let command = suzatary_io::mailbox::indexed_by(
            Cmd::new(&self.notmuch)
                .args(["new", "--no-hooks"])
                .timeout(REINDEX),
            self.config.as_deref(),
        );
        succeed(&command)
            .map(drop)
            .map_err(|failed| failed.to_string())
    }
}

impl MailCache {
    /// The account's kept mail deleted, and the index told.
    fn mail(&self, account: &MailAccount) -> Forgot {
        // Only a plain folder name is joined, so a delete never reaches outside the mail root.
        let Some(folder) = suzatary_core::mail::cache::folder(account) else {
            return Forgot::Failed {
                why: format!(
                    "{:?} is not a folder its mail could be kept in",
                    account.subdir
                ),
            };
        };
        match std::fs::remove_dir_all(self.root.join(folder)).present() {
            Ok(None) => Forgot::NotKept,
            Err(failed) => Forgot::Failed {
                why: failed.to_string(),
            },
            // Before the tap answers: a message the index still names but the maildir lacks
            // makes `notmuch show` fail, which every mail reading would answer as the box's.
            Ok(Some(())) => match self.reindex() {
                Ok(()) => Forgot::Deleted {
                    folder: folder.to_owned(),
                },
                Err(why) => Forgot::DeletedIndexStale {
                    folder: folder.to_owned(),
                    why,
                },
            },
        }
    }
}

impl suzatary_core::mail::cache::Forget for MailCache {
    fn forget(&self, account: &MailAccount, removal: Removal) -> Forgotten {
        let _held = match suzatary_io::durable::hold(&self.lock) {
            Ok(held) => held,
            Err(held) => {
                return Forgotten::failed(removal, &format!("the sync's hold: {held}"));
            }
        };
        match removal {
            Removal::SignIn => Forgotten::SignIn(revoked(&self.tokens, account)),
            Removal::Mail => Forgotten::Mail(self.mail(account)),
            Removal::Both => Forgotten::Both(revoked(&self.tokens, account), self.mail(account)),
        }
    }
}

/// An account's sign-ins revoked from `tokens`.
fn revoked(
    tokens: &suzatary_io::oauth::TokenStore,
    account: &MailAccount,
) -> Result<suzatary_core::mail::grants::Revoked, String> {
    use suzatary_core::mail::grants::Revoked;
    tokens
        .revoke(account)
        .map(|held| {
            if held {
                Revoked::Deleted
            } else {
                Revoked::NoneKept
            }
        })
        .map_err(|failed| failed.to_string())
}

/// A server that keeps no mail, as a test's is, and the sign-ins `tokens` keeps, if any.
pub struct Unkept(pub Option<suzatary_io::oauth::TokenStore>);

impl suzatary_core::mail::cache::Forget for Unkept {
    fn forget(&self, account: &MailAccount, removal: Removal) -> Forgotten {
        let grants = || match &self.0 {
            Some(tokens) => revoked(tokens, account),
            None => Ok(suzatary_core::mail::grants::Revoked::NoneKept),
        };
        let mail = || Forgot::Failed {
            why: "this server keeps no mail".to_owned(),
        };
        match removal {
            Removal::SignIn => Forgotten::SignIn(grants()),
            Removal::Mail => Forgotten::Mail(mail()),
            Removal::Both => Forgotten::Both(grants(), mail()),
        }
    }
}

#[cfg(test)]
mod tests {
    use suzatary_core::mail::account::Sync;
    use suzatary_core::mail::cache::Forget as _;

    use super::*;

    fn account(subdir: &str) -> MailAccount {
        MailAccount {
            email: suzatary_core::mail::account_email::AccountEmail::new("ada@example.test")
                .expect("a lower-case address"),
            subdir: subdir.to_owned(),
            sync: Sync::Gmail,
        }
    }

    /// One account's folder goes and its neighbours stay; a name that could reach outside the
    /// root deletes nothing; a second delete finds nothing kept; and an index that could not be
    /// told is said, the folder gone all the same.
    #[test]
    fn forgetting_deletes_one_account_s_folder_and_nothing_else() {
        let root = tempfile::tempdir().expect("a scratch root");
        for folder in [
            "ada-at-example-test",
            "bo-at-example-test",
            "cy-at-example-test",
        ] {
            std::fs::create_dir_all(root.path().join(folder).join("cur")).expect("a maildir");
        }
        let lock = root.path().join("state").join("sync.lock");
        let tokens = || suzatary_io::oauth::TokenStore::new(root.path().join("oauth"));
        let cache = MailCache::new(
            root.path().to_path_buf(),
            "true".into(),
            None,
            lock.clone(),
            tokens(),
        );
        let gone =
            |cache: &MailCache, subdir: &str| match cache.forget(&account(subdir), Removal::Mail) {
                Forgotten::Mail(forgot) => forgot,
                other => panic!("its mail alone was asked for: {other:?}"),
            };
        assert_eq!(
            gone(&cache, "ada-at-example-test"),
            Forgot::Deleted {
                folder: "ada-at-example-test".to_owned()
            }
        );
        assert!(!root.path().join("ada-at-example-test").exists());
        assert!(root.path().join("bo-at-example-test").exists());
        assert!(matches!(gone(&cache, ".."), Forgot::Failed { .. }));
        assert!(root.path().join("bo-at-example-test").exists());
        assert_eq!(gone(&cache, "ada-at-example-test"), Forgot::NotKept);
        let stale = MailCache::new(
            root.path().to_path_buf(),
            "false".into(),
            None,
            lock,
            tokens(),
        );
        assert!(matches!(
            gone(&stale, "cy-at-example-test"),
            Forgot::DeletedIndexStale { .. }
        ));
        assert!(!root.path().join("cy-at-example-test").exists());
    }
}
