//! Each account's calendars, read through the provider that keeps them (Google's Calendar API, or
//! Microsoft Graph) into the calendar's cache tier, by `suzatary plugin sync` beside the plugins.

use suzatary_base::capability::Clock as _;
use suzatary_google::calendar::{Occurrence, Provider};
use suzatary_health::health::Failure;
use suzatary_health::health::SourceKey;
use suzatary_health::health::{Healths, Outcome};
use suzatary_io_mail::calendar::ReadError;
use suzatary_io_mail::oauth::TokenStore;
use suzatary_mail::mail::oauth::{Calendar, NeedsLogin, OauthError, Protocol as _, Silent};

use crate::instance::Instance;

/// What reads an account's calendars: Google's grants, and the token store Graph's come from.
#[derive(Clone, Copy)]
pub struct Providers<'a> {
    pub google: &'a suzatary_io_mail::google::Grants,
    pub microsoft: &'a TokenStore,
}

/// Sync the calendars of every account the box reads them for: one line each, and whether every
/// one was stored.
#[must_use]
pub fn sync_calendars(
    instance: &Instance,
    google: suzatary_io_mail::google::Google,
    microsoft: &TokenStore,
    health: &suzatary_io_local::source_health::Health,
) -> (Vec<String>, bool) {
    let grants =
        suzatary_io_mail::google::Grants::new(google, std::sync::Arc::new(instance.secrets()));
    let accounts: Vec<&str> = (instance.mail.accounts.accounts().iter())
        .map(|account| account.subdir.as_str())
        .collect();
    let removed = remove_unowned(&instance.google_calendars(), &accounts);
    let (mut lines, healths, stored) = read_calendars(
        instance.mail.accounts.accounts(),
        Providers {
            google: &grants,
            microsoft,
        },
        |account| instance.google_calendar(account),
        suzatary_io_local::clock::SystemClock.now(),
    );
    match removed {
        Ok(0) => {}
        Ok(removed) => lines.push(format!(
            "calendars: removed those of {removed} account(s) no longer read"
        )),
        Err(failed) => lines.push(format!(
            "calendars: a removed account's calendar was not removed: {failed}"
        )),
    }
    if let Err(failed) = health.join(&healths) {
        lines.push(format!("calendars: the health was not recorded: {failed}"));
    }
    (lines, stored)
}

/// Empty each account's store held at another schema, before any fetch; then a line and a health
/// for each account the box reads calendars for (`Provider::held`), and whether every one was
/// stored.
pub fn read_calendars(
    accounts: &[suzatary_mail::mail::account::MailAccount],
    providers: Providers<'_>,
    store_of: impl Fn(&suzatary_mail::mail::account::MailAccount) -> std::path::PathBuf,
    now: suzatary_base::capability::Datetime,
) -> (Vec<String>, Healths<SourceKey>, bool) {
    let at = prodrome_data::instant::instant_of(now);
    let mut healths = Healths::default();
    let mut stored = true;
    let mut lines = Vec::new();
    for account in accounts {
        if let Err(failed) = settle(&store_of(account)) {
            stored = false;
            lines.push(format!(
                "calendars {}: its store was not emptied: {failed}",
                account.email
            ));
        }
        // A store that does not answer reads as no grant: the account's next sync says why.
        let google = providers
            .google
            .held(account.email.address())
            .unwrap_or(suzatary_google::google::Held::NotSignedIn);
        let microsoft = providers
            .microsoft
            .granted(account)
            .is_ok_and(|slots| slots.contains(&Calendar::SLOT));
        for provider in Provider::held(&account.sync, &google, microsoft) {
            let said = occurrences(provider, account, providers, at).and_then(|occurrences| {
                suzatary_io_mail::calendar::learn(&store_of(account), provider, &occurrences, now)
                    .map(|learnt| {
                        format!(
                            "{} occurrences read, {} learned, {} known",
                            occurrences.len(),
                            learnt.written,
                            learnt.known
                        )
                    })
                    .map_err(|why| (Failure::Unclassified, why))
            });
            let (outcome, words) = said.map_or_else(
                |(class, why)| (Outcome::Failed(class), why),
                |read| (Outcome::Succeeded, read),
            );
            stored &= outcome == Outcome::Succeeded;
            let key = SourceKey::GoogleCalendar(account.email.clone());
            healths = healths.join(&Healths::one(key, at, outcome, &words));
            lines.push(format!("{} {}: {words}", provider.actor(), account.email));
        }
    }
    (lines, healths, stored)
}

/// What `provider` reads of `account`'s calendars, or the class and words of why it could not.
fn occurrences(
    provider: Provider,
    account: &suzatary_mail::mail::account::MailAccount,
    providers: Providers<'_>,
    at: chrono::NaiveDateTime,
) -> Result<Vec<Occurrence>, (Failure, String)> {
    let read_error = |failed: ReadError| match failed {
        ReadError::NeedsLogin(_) => (Failure::NeedsLogin, failed.to_string()),
        ReadError::Failed(why) => (Failure::Unclassified, why),
    };
    match provider {
        Provider::Google => {
            let grant = (providers.google.live(account.email.address()))
                .map_err(|failed| (failed.class().into(), failed.to_string()))?;
            if !grant
                .parts
                .contains(suzatary_google::google::Scope::Calendar)
            {
                return Err((
                    Failure::NeedsLogin,
                    "its grant holds no calendars: a sign-in card asks Google for them".to_owned(),
                ));
            }
            providers
                .google
                .google()
                .occurrences(&grant, at)
                .map_err(read_error)
        }
        Provider::Microsoft => match providers.microsoft.access::<Calendar>(account) {
            Ok(Silent::Token(token)) => providers
                .microsoft
                .graph()
                .occurrences(&token, account.email.address(), at)
                .map_err(read_error),
            // The administrator's refusal is its own class: no sign-in and no retry mends it.
            Ok(Silent::NeedsLogin(needs @ NeedsLogin::AdminApproval { .. })) => {
                Err((Failure::NeedsAdministrator, needs.to_string()))
            }
            Ok(Silent::NeedsLogin(needs)) => Err((Failure::NeedsLogin, needs.to_string())),
            Err(failed @ OauthError::AdminApproval { .. }) => {
                Err((Failure::NeedsAdministrator, failed.to_string()))
            }
            Err(failed) => Err((Failure::Unclassified, failed.to_string())),
        },
    }
}

/// Empty the calendar store at `root` where it holds another schema, before any fetch.
///
/// # Errors
/// The calendar's schema refused, or the store not emptied.
fn settle(root: &std::path::Path) -> Result<(), String> {
    let schema = prodrome_declared::Declaration::admit(suzatary_google::calendar::SCHEMA)
        .map_err(|refused| format!("the calendar's schema: {refused}"))?;
    suzatary_io_chains::plugin_store::PluginStore::at(root)
        .settle(&schema)
        .map_err(|failed| failed.to_string())
}

/// Remove each directory under `root` that none of `names` names (a store no sync begins again:
/// a removed account's calendar, an undeclared plugin's); answers how many.
///
/// # Errors
/// `root` unlisted, or a directory not removed.
pub(crate) fn remove_unowned(root: &std::path::Path, names: &[&str]) -> Result<usize, String> {
    let failed = |why: std::io::Error| format!("{}: {why}", root.display());
    let Some(entries) =
        suzatary_os::present::Present::present(std::fs::read_dir(root)).map_err(failed)?
    else {
        return Ok(0);
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let owned = names.iter().any(|name| entry.file_name() == **name);
        if !owned && entry.file_type().map_err(failed)?.is_dir() {
            std::fs::remove_dir_all(entry.path()).map_err(failed)?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removed_accounts_calendar_store_is_removed() {
        let dir = suzatary_testkit::scratch("calendar-accounts");
        for account in ["ada-at-example-test", "bo-at-example-test"] {
            std::fs::create_dir_all(dir.path().join(account).join("store/objects"))
                .expect("a store");
        }
        std::fs::write(dir.path().join("note"), b"not a store").expect("writes");
        assert_eq!(remove_unowned(dir.path(), &["ada-at-example-test"]), Ok(1));
        assert!(dir.path().join("ada-at-example-test/store").exists());
        assert!(!dir.path().join("bo-at-example-test").exists());
        assert!(
            dir.path().join("note").exists(),
            "only directories are stores"
        );
        assert_eq!(
            remove_unowned(&dir.path().join("none"), &[]),
            Ok(0),
            "no directory, nothing to remove"
        );
    }
}
