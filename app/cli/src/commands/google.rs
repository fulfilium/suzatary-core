//! Each Google account's calendars, and each lieer account's that holds a calendar grant, read
//! through Google's Calendar API into the calendar's cache tier, by `suzatary plugin sync` beside
//! the plugins.

use suzatary_base::capability::Clock as _;
use suzatary_health::health::Failure;
use suzatary_health::health::SourceKey;
use suzatary_health::health::{Healths, Outcome};
use suzatary_io::google::ReadError;

use crate::instance::Instance;

/// Sync the calendars of every account the box reads them for, asking `google`: one line each,
/// and whether every one was stored.
#[must_use]
pub fn sync_calendars(
    instance: &Instance,
    google: suzatary_io::google::Google,
    health: &suzatary_io_local::source_health::Health,
) -> (Vec<String>, bool) {
    let grants = suzatary_io::google::Grants::new(google, std::sync::Arc::new(instance.secrets()));
    let accounts: Vec<&str> = (instance.mail.accounts.accounts().iter())
        .map(|account| account.subdir.as_str())
        .collect();
    let removed = remove_unowned(&instance.google_calendars(), &accounts);
    let (mut lines, healths, stored) = read_calendars(
        instance.mail.accounts.accounts(),
        &grants,
        |account| instance.google_calendar(account),
        suzatary_io_local::clock::SystemClock.now(),
    );
    match removed {
        Ok(0) => {}
        Ok(removed) => lines.push(format!(
            "google: removed the calendars of {removed} account(s) no longer read"
        )),
        Err(failed) => lines.push(format!(
            "google: a removed account's calendar was not removed: {failed}"
        )),
    }
    if let Err(failed) = health.join(&healths) {
        lines.push(format!(
            "google: the calendars' health was not recorded: {failed}"
        ));
    }
    (lines, stored)
}

/// Empty each account's store held at another schema, before any fetch; then a line and a health
/// for each account the box reads calendars for (`google::reads_calendars`), and whether every one
/// was stored; a lieer account is read through its calendar grant alone.
pub fn read_calendars(
    accounts: &[suzatary_mail::mail::account::MailAccount],
    grants: &suzatary_io::google::Grants,
    store_of: impl Fn(&suzatary_mail::mail::account::MailAccount) -> std::path::PathBuf,
    now: suzatary_base::capability::Datetime,
) -> (Vec<String>, Healths<SourceKey>, bool) {
    let at = prodrome_core::fpl::instant_of(now);
    let mut healths = Healths::default();
    let mut stored = true;
    let mut lines = Vec::new();
    for account in accounts {
        if let Err(failed) = settle(&store_of(account)) {
            stored = false;
            lines.push(format!(
                "google {}: its calendar store was not emptied: {failed}",
                account.email
            ));
        }
        // A store that does not answer reads as no grant: the account's next sync says why.
        let held = grants
            .held(account.email.address())
            .unwrap_or(suzatary_google::google::Held::NotSignedIn);
        if !suzatary_google::google::reads_calendars(&account.sync, &held) {
            continue;
        }
        let said = (grants.live(account.email.address()))
            .map_err(|failed| (failed.class().into(), failed.to_string()))
            .and_then(|grant| {
                if !grant
                    .parts
                    .contains(suzatary_google::google::Scope::Calendar)
                {
                    return Err((
                        Failure::NeedsLogin,
                        "its grant holds no calendars: a sign-in card asks Google for them"
                            .to_owned(),
                    ));
                }
                let read = grants.google().occurrences(&grant, at);
                read.map_err(|failed| match failed {
                    ReadError::NeedsLogin(_) => (Failure::NeedsLogin, failed.to_string()),
                    ReadError::Failed(why) => (Failure::Unclassified, why),
                })
            })
            .and_then(|occurrences| {
                suzatary_io::google::learn(&store_of(account), &occurrences, now)
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
        lines.push(format!("google {}: {words}", account.email));
    }
    (lines, healths, stored)
}

/// Empty the calendar store at `root` where it holds another schema, before any fetch.
///
/// # Errors
/// The calendar's schema refused, or the store not emptied.
fn settle(root: &std::path::Path) -> Result<(), String> {
    let schema = prodrome_core::declared::Declaration::admit(suzatary_google::google::SCHEMA)
        .map_err(|refused| format!("the calendar's schema: {refused}"))?;
    suzatary_io::plugin_store::PluginStore::at(root)
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
