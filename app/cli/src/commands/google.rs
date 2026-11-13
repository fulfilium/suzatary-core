//! Each Google account's calendars, and each lieer account's that holds a calendar grant, read
//! through Google's Calendar API into the calendar's cache tier: what `suzatary plugin sync` runs beside the plugins, on the same timer. Each
//! account's result joins the sources' health under its calendars' key.

use suzatary_base::capability::Clock as _;
use suzatary_core::health::Failure;
use suzatary_core::health::SourceKey;
use suzatary_core::health::{Healths, Outcome};
use suzatary_io::google::ReadError;

use crate::instance::Instance;

/// Sync the calendars of every account the box reads them for, asking `google`: one line each,
/// and whether every one was stored.
#[must_use]
pub fn sync_calendars(
    instance: &Instance,
    google: suzatary_io::google::Google,
    health: &suzatary_io::source_health::Health,
) -> (Vec<String>, bool) {
    let grants = suzatary_io::google::Grants::new(google, std::sync::Arc::new(instance.secrets()));
    let (mut lines, healths, stored) = read_calendars(
        instance.mail.accounts.accounts(),
        &grants,
        |account| instance.google_calendar(account),
        suzatary_io::clock::SystemClock.now(),
    );
    if let Err(failed) = health.join(&healths) {
        lines.push(format!(
            "google: the calendars' health was not recorded: {failed}"
        ));
    }
    (lines, stored)
}

/// Read into `store_of` the calendars of each of `accounts` the box reads them for
/// (`google::reads_calendars`): a line and a health each, and whether every one was stored. A
/// lieer account is read through its calendar grant alone, and its mail stays lieer's.
pub fn read_calendars(
    accounts: &[suzatary_core::mail::account::MailAccount],
    grants: &suzatary_io::google::Grants,
    store_of: impl Fn(&suzatary_core::mail::account::MailAccount) -> std::path::PathBuf,
    now: suzatary_base::capability::Datetime,
) -> (Vec<String>, Healths<SourceKey>, bool) {
    let at = prodrome_core::fpl::instant_of(now);
    let mut healths = Healths::default();
    let mut stored = true;
    let mut lines = Vec::new();
    for account in accounts {
        // A store that does not answer reads as no grant: the account's next sync says why.
        let held = grants
            .held(account.email.address())
            .unwrap_or(suzatary_core::google::Held::NotSignedIn);
        if !suzatary_core::google::reads_calendars(&account.sync, &held) {
            continue;
        }
        let said = (grants.live(account.email.address()))
            .map_err(|failed| (failed.class().into(), failed.to_string()))
            .and_then(|grant| {
                if !grant.parts.contains(suzatary_core::google::Scope::Calendar) {
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
