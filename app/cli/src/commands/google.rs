//! Each Google account's calendars, read through Google's Calendar API into the calendar's
//! cache tier: what `suzatary plugin sync` runs beside the plugins, on the same timer. Each
//! account's result joins the sources' health under its calendars' key.

use suzatary_core::capability::Clock as _;
use suzatary_core::connect::Failure;
use suzatary_core::health::{Healths, Outcome};
use suzatary_core::mail::account::Sync;
use suzatary_core::source_key::SourceKey;
use suzatary_io::google::CalendarError;

use crate::instance::Instance;

/// Sync every Google account's calendars: one line each, and whether every one was stored.
#[must_use]
pub fn sync_calendars(
    instance: &Instance,
    health: &suzatary_io::source_health::Health,
) -> (Vec<String>, bool) {
    let grants = super::mail::google_grants(instance);
    let now = suzatary_io::clock::SystemClock.now();
    let at = prodrome_core::fpl::instant_of(now);
    let mut healths = Healths::default();
    let mut stored = true;
    let mut lines = Vec::new();
    for account in instance.mail.accounts.accounts() {
        if !matches!(account.sync, Sync::Google) {
            continue;
        }
        let said = (grants.live(account.email.address()))
            .map_err(|failed| (failed.class(), failed.to_string()))
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
                    CalendarError::NeedsLogin(_) => (Failure::NeedsLogin, failed.to_string()),
                    CalendarError::Failed(why) => (Failure::Unclassified, why),
                })
            })
            .and_then(|occurrences| {
                suzatary_io::google::learn(&instance.google_calendar(account), &occurrences, now)
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
    if let Err(failed) = health.join(&healths) {
        lines.push(format!(
            "google: the calendars' health was not recorded: {failed}"
        ));
    }
    (lines, stored)
}
