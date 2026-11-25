//! The rule over the connections' health: what a failing connection is owed, by its failure's
//! class, and what an unused one is offered, filed by the box with no model asked.
//!
//! A fix is a function of the connection and the instant its failure began, so the same failure
//! asks for the same card and a failure after a success is a new card.

use std::collections::BTreeSet;

use chrono::Duration;

use crate::secretary::bootstrap::Answering;
use crate::secretary::noticed::Owed;
use suzatary_connect::connect::{Connection, Declared, Key, Step, Table};
use suzatary_connect::settings::{Settable, Settables};
use suzatary_google::calendar::CalendarName;
use suzatary_health::health::SourceKey;
use suzatary_health::health::{Failure, Remedy};
use suzatary_health::health::{Health, Healths, Instant, Reading};
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::proposal::Proposal;

/// How long a calendar may hold nothing, before today and after it, before it is offered for
/// removal.
#[must_use]
pub fn quiet() -> Duration {
    Duration::days(i64::from(QUIET))
}

/// [`quiet`], in days.
pub const QUIET: u32 = 90;

/// How long a mail account may fail before it is offered for removal.
#[must_use]
pub(crate) fn abandoned() -> Duration {
    Duration::days(ABANDONED)
}

/// [`abandoned`], in days.
pub(crate) const ABANDONED: i64 = 7;

#[derive(Clone, Copy)]
pub struct Observed<'a> {
    pub declared: &'a Declared,
    pub answering: &'a Answering<'a>,
    pub health: &'a Healths<SourceKey>,
    /// The calendars holding an occurrence within [`quiet`] of `now`; `None` (unreadable) holds
    /// every waiting card, since a calendar still quiet cannot be told from one that is not.
    pub heard: Option<&'a BTreeSet<CalendarName>>,
    pub now: Instant,
}

/// One failing observation: what failed, of what class, since when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failing {
    /// The connection that failed, one the instance's tables hold.
    pub connection: Connection,
    pub class: Failure,
    /// It has not succeeded since this instant.
    pub since: Instant,
}

impl Failing {
    /// The one card this failure is owed: its class's remedy as a step of its own template; none
    /// for a remedy no tap gives, or one its connection has no step for.
    #[must_use]
    pub fn fix(&self, settables: &Settables) -> Option<Owed> {
        let episode = prodrome_data::instant::datetime_of(self.since).ok()?;
        let since = utc(self.since);
        let connection = &self.connection;
        let (act, instead): (Act, Option<&str>) = match self.remedy() {
            Remedy::Retry => return None,
            Remedy::Secret => {
                let name = connection.secrets().into_iter().next()?;
                let act = Act::Secret {
                    name: Settable::mint(settables, &name).ok()?.name().to_owned(),
                    // The same words whatever the class, so a class moving within one
                    // episode (an outage, then 401) asks for the one card.
                    why: format!(
                        "{} has failed since {since}: store it again",
                        connection.called()
                    ),
                };
                (act, None)
            }
            Remedy::Auth => connection
                .steps()
                .into_iter()
                .find_map(|step| match step {
                    Step::DeviceCode { account } => Some(Act::Auth { account }),
                    // A revoked grant, or one a client in Testing ended after 7 days, holds no
                    // part any more (Google forgets the whole grant), so the consent asks for all.
                    Step::AuthCode { account } => Some(Act::GoogleSignIn {
                        account,
                        scopes: suzatary_google::google::Scopes::all(),
                    }),
                    Step::Secret { .. }
                    | Step::SendingConsent { .. }
                    | Step::GoogleClient
                    | Step::SignIn
                    | Step::Terminal { .. } => None,
                })
                .map(|act| (act, None))
                // Lieer's own sign-in is a terminal step, so a Gmail account is offered the move
                // onto the box's own Google sign-in, which a card can finish.
                .or_else(|| {
                    let google = connection.moved_to_google()?;
                    Some((Act::Connect { connection: google }, Some(MOVE)))
                })?,
        };
        Some(
            Owed::one(
                Proposal::Act(act),
                format!(
                    "{} has failed since {since}: {}{}",
                    self.named(),
                    said(self.class),
                    instead.unwrap_or_default()
                ),
            )
            .since(episode),
        )
    }

    fn named(&self) -> String {
        self.connection.called()
    }
}

impl Failing {
    /// A link is always set again from its Secret card, a valid move whatever its failure;
    /// everything else takes its class's one remedy.
    fn remedy(&self) -> Remedy {
        match self.connection.template().table {
            Table::Link(_) => Remedy::Secret,
            Table::Always
            | Table::Model
            | Table::Account(_)
            | Table::Folder
            | Table::Channel(_) => self.class.remedy(),
        }
    }
}

/// What a lieer account's card adds: the move is how its sign-in is renewed.
const MOVE: &str = "; this card signs it in with Google instead, which reads the same mail and \
                    its calendars too";

fn capitalised(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// An instant as a card says it: `2026-10-07 09:00 UTC`.
fn utc(at: Instant) -> String {
    format!("{} UTC", at.format("%Y-%m-%d %H:%M"))
}

/// The line the box says above a card it files, never the card's own identity.
fn said(class: Failure) -> String {
    match class {
        Failure::Revoked { status } => format!(
            "it answered {}, so its link was changed or revoked",
            status.status()
        ),
        Failure::NeedsLogin => "it needs its sign-in again".to_owned(),
        Failure::Unresolved => "it reads a secret stored nowhere".to_owned(),
        Failure::KeyRefused => "its key was refused".to_owned(),
        Failure::Limited(limited) => limited.to_string(),
        Failure::Unusable => "the link stored for it is not one the box sends".to_owned(),
        Failure::Unreachable => {
            "its address could not be reached: check the link and set it again".to_owned()
        }
        Failure::Unclassified => "it failed".to_owned(),
    }
}

/// Each declared connection one of whose sources reads as failing; of two failing sources, one
/// whose class a tap mends is the one owed a card, so a calendar's outage never hides its
/// account's sign-in; of two alike, the longer failing. A model that does not answer is the
/// bootstrap's to ask for (`bootstrap::owed`), so its credential is not in this rule.
#[must_use]
pub fn failing(observed: &Observed<'_>) -> Vec<Failing> {
    observed
        .declared
        .connections()
        .values()
        .filter_map(|connection| {
            let (class, since) = connection
                .sources()
                .iter()
                .filter_map(|key| observed.health.health(key).and_then(Health::remedial))
                .min_by_key(|(class, since)| (class.remedy() == Remedy::Retry, *since))?;
            Some(Failing {
                connection: connection.clone(),
                class,
                since,
            })
        })
        .collect()
}

/// The cards the connections' health owes; a secret or a sign-in another rule also asks for is
/// one need, so joined after that rule's cards it is that rule's card ([`Owed::join`]).
#[must_use]
pub fn owed(observed: &Observed<'_>) -> Owed {
    let settables = observed.answering.settables;
    let failing = failing(observed);
    let fixes = failing
        .iter()
        .filter_map(|failing| failing.fix(settables))
        .fold(Owed::default(), Owed::join);
    fixes.join(unused(observed, &failing))
}

/// The removals an unused connection is offered: an account whose mail has failed for
/// [`abandoned`] (its calendars failing alone leave its mail in use), and a calendar that syncs
/// and is [`quiet`]; a failure a tap mends has its card, so its connection is offered no removal,
/// the opposite act for the same state.
fn unused(observed: &Observed<'_>, failing: &[Failing]) -> Owed {
    let mended = |key: Key| {
        failing.iter().any(|failing| {
            failing.connection.key() == key && failing.class.remedy() != Remedy::Retry
        })
    };
    let accounts = failing.iter().filter_map(|failing| {
        let connection = &failing.connection;
        let mail = connection.sources().into_iter().find(|key| match key {
            SourceKey::Mail(_) => true,
            SourceKey::Plugin(_)
            | SourceKey::Link { .. }
            | SourceKey::GoogleCalendar(_)
            | SourceKey::Drive => false,
        })?;
        let since = match observed.health.reading(&mail)? {
            Reading::Failing { since, .. } if observed.now - since >= abandoned() => since,
            Reading::Failing { .. } | Reading::Ok { .. } => return None,
        };
        if mended(connection.key()) {
            return None;
        }
        Some(
            Owed::one(
                Proposal::Act(Act::Disconnect {
                    connection: connection.clone(),
                }),
                format!(
                    "{} has not synced since {}, {ABANDONED} days or more.",
                    connection.called(),
                    utc(since)
                ),
            )
            .since(prodrome_data::instant::datetime_of(since).ok()?),
        )
    });
    let calendars = match observed.heard {
        None => Owed::default().holding(),
        Some(heard) => observed
            .declared
            .connections()
            .values()
            .filter_map(|connection| quiet_calendar(observed, heard, connection))
            .fold(Owed::default(), Owed::join),
    };
    accounts.fold(Owed::default(), Owed::join).join(calendars)
}

/// The removal a calendar is offered when its link syncs, one that does not fail, and its store
/// holds nothing within [`quiet`] of now: a calendar that holds nothing because its link fails is
/// offered none.
fn quiet_calendar(
    observed: &Observed<'_>,
    heard: &BTreeSet<CalendarName>,
    connection: &Connection,
) -> Option<Owed> {
    match connection.template().table {
        Table::Link(_) => {}
        Table::Always | Table::Model | Table::Account(_) | Table::Folder | Table::Channel(_) => {
            return None;
        }
    }
    let name = connection.secrets().into_iter().next()?;
    let synced = (connection.sources().iter())
        .filter_map(|key| match observed.health.reading(key)? {
            Reading::Ok { at } => Some(at),
            Reading::Failing { .. } => None,
        })
        .max()?;
    if heard.iter().any(|calendar| calendar.as_str() == name) {
        return None;
    }
    Some(Owed::one(
        Proposal::Act(Act::Disconnect {
            connection: connection.clone(),
        }),
        format!(
            "{} synced last at {} and holds nothing within {QUIET} days of today.",
            capitalised(&connection.called()),
            utc(synced)
        ),
    ))
}
