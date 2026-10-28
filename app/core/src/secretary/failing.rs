//! The rule over the connections' health: what a failing connection is owed, by its failure's
//! class, and what an unused one is offered, filed by the box with no model asked.
//!
//! This generalises `mail_health`'s identity rule to every connection: a fix is a function of
//! the connection and the instant its failure began (its last success, or its first result), so
//! seeing the same failure again asks for the same card, which stays as it was answered, and a
//! failure after a success is a new episode with a new card. A class has one remedy
//! ([`Failure::remedy`]); the card is that remedy's step of the connection's own template,
//! except that a failing link is always owed its Secret card, worded by its class, since
//! setting a link again is a valid move whatever went wrong.
//!
//! What goes unused is offered for removal, each a card the owner may dismiss: a calendar whose
//! link syncs and holds nothing within [`QUIET`] of today, and a mail account failing for
//! [`ABANDONED`]. A connection whose failure a card already mends is offered no removal beside
//! that card.

use std::collections::BTreeSet;

use chrono::Duration;

use crate::calendar::CalendarName;
use crate::connect::{Connection, Declared, Failure, Key, Remedy, Step, Table};
use crate::health::{Health, Healths, Instant, Reading};
use crate::secretary::acts::Act;
use crate::secretary::bootstrap::Answering;
use crate::secretary::noticed::Owed;
use crate::secretary::store::Proposal;
use crate::settings::{Settable, Settables};
use crate::source_key::SourceKey;

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
pub fn abandoned() -> Duration {
    Duration::days(ABANDONED)
}

/// [`abandoned`], in days.
pub const ABANDONED: i64 = 7;

/// What the rule reads.
#[derive(Clone, Copy)]
pub struct Observed<'a> {
    /// The tables connections are read from.
    pub declared: &'a Declared,
    /// What the model answers, and every secret name declared.
    pub answering: &'a Answering<'a>,
    /// Every source's results.
    pub health: &'a Healths<SourceKey>,
    /// The calendars holding an occurrence within [`quiet`] of `now`; `None` when the
    /// calendars could not be read, which holds every waiting card, since a calendar still
    /// quiet cannot be told from one that is not.
    pub heard: Option<&'a BTreeSet<CalendarName>>,
    /// Now, UTC.
    pub now: Instant,
}

/// One failing observation: what failed, of what class, since when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failing {
    /// The connection that failed, one the instance's tables hold.
    pub connection: Connection,
    /// Its class.
    pub class: Failure,
    /// It has not succeeded since this instant.
    pub since: Instant,
}

impl Failing {
    /// The one card this failure is owed: its class's remedy as a step of its own template
    /// (the link's secret, the account's sign-in); none for a remedy no tap gives, or one its
    /// connection has no step for.
    #[must_use]
    pub fn fix(&self, settables: &Settables) -> Option<Owed> {
        let episode = prodrome_core::fpl::datetime_of(self.since).ok()?;
        let since = utc(self.since);
        let connection = &self.connection;
        let act = match self.remedy() {
            Remedy::Retry => return None,
            Remedy::Secret => {
                let name = connection.secrets().into_iter().next()?;
                Act::Secret {
                    name: Settable::mint(settables, &name).ok()?.name().to_owned(),
                    // The same words whatever the class, so a class moving within one
                    // episode (an outage, then 401) asks for the one card; the class is said
                    // in the line above it.
                    why: format!(
                        "{} has failed since {since}: store it again",
                        connection.called()
                    ),
                }
            }
            Remedy::Auth => {
                connection.steps().into_iter().find_map(|step| match step {
                    Step::DeviceCode { account } => Some(Act::Auth { account }),
                    // A revoked grant, or one a client in Testing ended after 7 days, holds no
                    // part any more (Google forgets the whole grant), so every part is missing
                    // and the consent asks for them all again.
                    Step::AuthCode { account } => Some(Act::GoogleSignIn {
                        account,
                        scopes: crate::google::Scopes::all(),
                    }),
                    Step::Secret { .. }
                    | Step::SendingConsent { .. }
                    | Step::GoogleClient
                    | Step::SignIn
                    | Step::Terminal { .. } => None,
                })?
            }
        };
        Some(
            Owed::one(
                Proposal::Act(act),
                format!(
                    "{} has failed since {since}: {}",
                    self.named(),
                    said(self.class)
                ),
            )
            .since(episode),
        )
    }

    /// What failed, as the owner calls it.
    fn named(&self) -> String {
        self.connection.called()
    }
}

impl Failing {
    /// What mends its class here: a link is always set again from its Secret card, since
    /// setting it again is a valid move whatever its failure (a mistyped address, a rotated
    /// link, a service gone); everything else takes its class's one remedy.
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

/// `words` begun with a capital, as a sentence begins.
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

/// What a class of failure said, in the box's words: the line the box says above a card it
/// files, never the card's own identity.
fn said(class: Failure) -> String {
    match class {
        Failure::Revoked { status } => format!(
            "it answered {}, so its link was changed or revoked",
            status.status()
        ),
        Failure::NeedsLogin => "it needs its sign-in again".to_owned(),
        Failure::Unresolved => "it reads a secret stored nowhere".to_owned(),
        Failure::KeyRefused => "its key was refused".to_owned(),
        Failure::Unusable => "the link stored for it is not one the box sends".to_owned(),
        Failure::Unreachable => {
            "its address could not be reached: check the link and set it again".to_owned()
        }
        Failure::Unclassified => "it failed".to_owned(),
    }
}

/// Every failing observation: each declared connection one of whose sources reads as failing,
/// of the class and since the instant of that source. Of two failing sources, one whose class a
/// tap mends is the one owed a card, so a calendar's outage never hides its account's sign-in;
/// of two alike, the longer failing. A model that does not answer is the bootstrap's to ask for
/// (`bootstrap::owed`).
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

/// The cards the connections' health owes: one per failing connection and episode whose remedy
/// a tap gives, and each removal an unused connection is offered. A secret or a sign-in another
/// rule also asks for is one need, so joined after that rule's cards it is that rule's card
/// ([`Owed::join`]).
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
/// [`abandoned`] (a Google account's calendars failing alone leave its mail in use), and a
/// calendar that syncs and is [`quiet`]. A failure a tap mends has its card, from this rule or
/// another, so its connection is offered no removal, which would be the opposite act for the
/// same state.
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
            .since(prodrome_core::fpl::datetime_of(since).ok()?),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Outcome;
    use crate::secretary::inbox::{self, Fold};
    use crate::secretary::inbox_event::InboxEvent;
    use crate::secretary::proposer::Proposer;
    use crate::store::InMemory;

    fn at(hour: u32) -> Instant {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
            .and_then(|day| day.and_hms_opt(0, 0, 0))
            .expect("an instant")
            + Duration::hours(i64::from(hour))
    }

    fn work() -> Connection {
        Connection::parse("ics", &["Work".to_owned()]).expect("a calendar")
    }

    fn account() -> Connection {
        Connection::parse(
            "imap-device-code",
            &["ada@example.test".to_owned(), "consumers".to_owned()],
        )
        .expect("an account")
    }

    fn google() -> Connection {
        Connection::parse("google", &["bea@example.test".to_owned()]).expect("a Google account")
    }

    /// A deployment declaring the Work calendar, Ada's Microsoft account and Bea's Google
    /// account.
    struct World {
        declared: Declared,
        settables: Settables,
        answered: Healths<String>,
        health: Healths<SourceKey>,
        heard: BTreeSet<CalendarName>,
    }

    impl World {
        fn new() -> Self {
            let declared = Declared::default()
                .with(&work())
                .and_then(|declared| declared.with(&account()))
                .and_then(|declared| declared.with(&google()))
                .expect("declared");
            Self {
                settables: Settables::derive(
                    crate::settings::MODEL_TOKEN,
                    [],
                    None,
                    &declared.plugins,
                ),
                declared,
                answered: Healths::default(),
                health: Healths::default(),
                heard: BTreeSet::new(),
            }
        }

        fn link(&mut self, at: Instant, outcome: Outcome) {
            let key = work()
                .sources()
                .into_iter()
                .next()
                .expect("a calendar reads its link");
            self.health = self
                .health
                .join(&Healths::one(key, at, outcome, "it answered"));
        }

        fn google(&mut self, key: &SourceKey, at: Instant, outcome: Outcome) {
            let one = Healths::one(key.clone(), at, outcome, "it failed");
            self.health = self.health.join(&one);
        }

        fn owed(&self, now: Instant) -> Owed {
            let resolves = |_: &Settable| true;
            let answering = Answering {
                model: "a-cli/a-model",
                health: &self.answered,
                settables: &self.settables,
                resolves: &resolves,
            };
            owed(&Observed {
                declared: &self.declared,
                answering: &answering,
                health: &self.health,
                heard: Some(&self.heard),
                now,
            })
        }
    }

    fn file(
        owed: &Owed,
        inbox_store: &InMemory<InboxEvent>,
        conversations: &crate::secretary::conversations::InMemory,
        hour: u32,
    ) {
        owed.file(
            conversations.conversations(),
            inbox_store,
            prodrome_core::fpl::datetime_of(at(hour)).expect("in range"),
        )
        .expect("filed");
    }

    fn secrets_filed(fold: &Fold) -> usize {
        fold.objects()
            .filter(|object| {
                object.asked.proposer == Proposer::Box
                    && matches!(&object.asked.proposal, Proposal::Act(Act::Secret { .. }))
            })
            .count()
    }

    /// One result of a link: it synced, or it failed of a refusing status, of an address not
    /// reached, or of an outage.
    #[derive(Debug, Clone, Copy)]
    enum Seen {
        Synced,
        Refused(u16),
        Unreached,
        Outage,
    }

    fn seen() -> impl proptest::strategy::Strategy<Value = Seen> {
        use proptest::prelude::*;
        prop_oneof![
            Just(Seen::Synced),
            proptest::sample::select(vec![401u16, 403, 404]).prop_map(Seen::Refused),
            Just(Seen::Unreached),
            Just(Seen::Outage),
        ]
    }

    proptest::proptest! {
        /// However often each failure is seen, whatever its class moves to between sightings
        /// (an outage, 403, then 404), and whatever the box restarts between filings (each
        /// reads the inbox afresh), one card is filed per episode of a failing link, an episode
        /// being every result since one success; and the card stays as it stood (dismissed stays
        /// dismissed, waiting stays waiting) until a success.
        #[test]
        fn one_act_per_connection_and_since(
            script in proptest::collection::vec((seen(), 1u8..4, proptest::bool::ANY), 1..20)
        ) {
            let mut world = World::new();
            let inbox_store = InMemory::<InboxEvent>::default();
            let conversations = crate::secretary::conversations::InMemory::default();
            // How many episodes began.
            let mut episodes = 0usize;
            let mut open_episode = false;
            let mut hour = 0;
            for (result, sightings, dismiss) in script {
                hour += 1;
                world.link(at(hour), outcome(result));
                match result {
                    Seen::Synced => open_episode = false,
                    Seen::Refused(_) | Seen::Unreached | Seen::Outage => {
                        // A failure after a success, or the first result, begins an episode.
                        if !open_episode {
                            episodes += 1;
                            open_episode = true;
                        }
                    }
                }
                for _ in 0..sightings {
                    file(&world.owed(at(hour)), &inbox_store, &conversations, hour);
                }
                let fold = inbox::fold(&inbox_store).expect("reads");
                let waiting = fold.objects().filter(|object| {
                    object.reading.open()
                        && matches!(&object.asked.proposal, Proposal::Act(Act::Secret { .. }))
                }).count();
                // Within an episode, its card waits until dismissed or a success.
                proptest::prop_assert!(waiting <= 1, "{} cards wait at once", waiting);
                if dismiss
                    && let Some(open) = fold.objects().find(|object| object.reading.open())
                {
                    let owner = prodrome_core::event::Actor::new("ada").expect("an actor");
                    let when = prodrome_core::fpl::datetime_of(at(hour)).expect("in range");
                    inbox::dismiss(&inbox_store, &open.id, &owner, when, "not now").expect("dismissed");
                }
            }
            let fold = inbox::fold(&inbox_store).expect("reads");
            proptest::prop_assert_eq!(secrets_filed(&fold), episodes);
            // A quiet calendar's removal is withdrawn once its link fails, since a fix card
            // stands then; only the fix cards are counted here.
            let withdrawn = fold.objects().filter(|object| {
                matches!(&object.asked.proposal, Proposal::Act(Act::Secret { .. }))
                && matches!(object.reading.shown().map(|answer| &answer.response),
                    Some(crate::secretary::responded::Responded::Withdrawn(_)))
            }).count();
            // Only an episode a success ended withdraws its card.
            proptest::prop_assert!(withdrawn <= episodes.saturating_sub(usize::from(open_episode)));
        }
    }

    /// A Google account reads two sources, its mail and its calendars. Its calendars failing
    /// alone for a week leave its mail in use, so no removal is offered; a sign-in its mail needs
    /// is owed its card whatever older outage its calendars have; and a sign-in card another
    /// rule already asks for, for one missing part, is that card.
    #[test]
    fn a_google_accounts_two_sources() {
        let bea =
            crate::mail::account_email::AccountEmail::new("bea@example.test").expect("an address");
        let (mail, calendars) = (
            SourceKey::Mail(bea.clone()),
            SourceKey::GoogleCalendar(bea.clone()),
        );
        let outage = Outcome::Failed(Failure::Unclassified);
        let count = |owed: &Owed, act: fn(&Act) -> bool| {
            owed.iter()
                .filter(|noticed| matches!(&noticed.proposal, Proposal::Act(held) if act(held)))
                .count()
        };
        let disconnects = |owed: &Owed| count(owed, |act| matches!(act, Act::Disconnect { .. }));
        let sign_ins = |owed: &Owed| count(owed, |act| matches!(act, Act::GoogleSignIn { .. }));
        let week = at(24 * 8);

        let mut world = World::new();
        world.google(&calendars, at(0), outage);
        world.google(&mail, week, Outcome::Succeeded);
        assert_eq!(disconnects(&world.owed(week)), 0, "its mail syncs");
        let mut failing = World::new();
        failing.google(&mail, at(1), outage);
        failing.google(&mail, week, outage);
        assert_eq!(disconnects(&failing.owed(week)), 1, "its mail fails a week");

        let mut lapsed = World::new();
        lapsed.google(&calendars, at(0), outage);
        lapsed.google(&mail, at(4), Outcome::Succeeded);
        lapsed.google(&mail, at(5), Outcome::Failed(Failure::NeedsLogin));
        assert_eq!(sign_ins(&lapsed.owed(at(6))), 1, "the mail's sign-in");

        let mut partial = World::new();
        partial.google(&calendars, at(1), Outcome::Failed(Failure::NeedsLogin));
        assert_eq!(sign_ins(&partial.owed(at(2))), 1, "owed when nothing asks");
        let asked = Owed::one(
            Proposal::Act(Act::GoogleSignIn {
                account: bea.address().clone(),
                scopes: crate::google::Scopes::of([crate::google::Scope::Calendar])
                    .expect("one part"),
            }),
            "it lacks its calendars".to_owned(),
        );
        let joined = asked.clone().join(partial.owed(at(2)));
        assert_eq!(sign_ins(&joined), 1, "one sign-in");
        assert!(
            joined
                .iter()
                .any(|noticed| asked.iter().any(|held| held.proposal == noticed.proposal)),
            "that card"
        );
    }

    fn outcome(result: Seen) -> Outcome {
        match result {
            Seen::Synced => Outcome::Succeeded,
            Seen::Refused(status) => Outcome::Failed(Failure::of_status(status)),
            Seen::Unreached => Outcome::Failed(Failure::Unreachable),
            Seen::Outage => Outcome::Failed(Failure::Unclassified),
        }
    }
}
