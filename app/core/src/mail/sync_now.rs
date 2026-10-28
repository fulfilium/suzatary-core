//! A mail sync asked for now: the answer, and when the last one is fresh enough to be it.
//!
//! A sync is a join: pulling a server into the maildir twice in a row holds what pulling it once
//! does. So asking again within [`WINDOW`] of a sync is answered by that sync, and starting one
//! is idempotent too, since the unit never runs twice at once.

use serde::Serialize;

use super::verbs::{AccountSync, SyncReport};
use crate::capability::{Datetime, Synced};

/// How recent a sync must be to answer a request for one without starting another.
pub const WINDOW: chrono::TimeDelta = chrono::TimeDelta::seconds(60);

/// What a request for a sync now answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "sync", rename_all = "snake_case")]
pub enum SyncNow {
    /// One finished within [`WINDOW`], so none was started; its report.
    Fresh(Report),
    /// It was started, and this is its report.
    Ran(Report),
    /// It was started and did not report within the wait; the pass carries on without us.
    Pending {
        /// When it was asked for.
        started: String,
        /// What to do next.
        note: &'static str,
    },
}

/// One sync's report, as much of it as the request asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    /// When it finished.
    pub at: String,
    /// The accounts asked about, each as it synced.
    pub accounts: Vec<AccountSync>,
    /// What `notmuch new` said of the whole pull.
    pub notmuch: String,
    /// Why the sync ran without its hold, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unheld: Option<String>,
}

impl Report {
    /// `synced`, narrowed to `account` when one was asked about.
    #[must_use]
    pub fn of(synced: &Synced, account: Option<&str>) -> Self {
        let SyncReport {
            synced: all,
            notmuch,
            unheld,
        } = &synced.report;
        Self {
            at: iso(synced.at),
            accounts: all
                .iter()
                .filter(|row| account.is_none_or(|asked| row.account.is(asked)))
                .cloned()
                .collect(),
            notmuch: notmuch.clone(),
            unheld: unheld.clone(),
        }
    }
}

/// The last sync, when it finished within [`WINDOW`] before `now`.
#[must_use]
pub fn fresh(last: Option<Synced>, now: Datetime) -> Option<Synced> {
    let now = prodrome_core::fpl::instant_of(now);
    last.filter(|synced| {
        let age = now - prodrome_core::fpl::instant_of(synced.at);
        age >= chrono::TimeDelta::zero() && age < WINDOW
    })
}

/// The pending answer for a sync asked for at `at`.
#[must_use]
pub fn pending(at: Datetime) -> SyncNow {
    SyncNow::Pending {
        started: iso(at),
        note: "The sync was started and had not reported yet. Ask again in a minute; the \
               answer will be its report.",
    }
}

fn iso(at: Datetime) -> String {
    prodrome_core::fpl::iso(prodrome_core::fpl::instant_of(at))
}
