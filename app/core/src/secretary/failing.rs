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

use std::collections::{BTreeMap, BTreeSet};

use chrono::Duration;

use crate::calendar::CalendarName;
use crate::connect::{Connection, Declared, Failure, Key, Remedy, Step, Table};
use crate::mail::health::{Health, Healths, Instant, Reading};
use crate::plugin::PluginName;
use crate::secretary::acts::Act;
use crate::secretary::bootstrap::Answering;
use crate::secretary::noticed::Owed;
use crate::secretary::store::Proposal;
use crate::settings::{Settable, Settables};

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
    /// Each mail account's syncs.
    pub mail: &'a Healths,
    /// Each plugin's links' fetches, by plugin.
    pub links: &'a BTreeMap<PluginName, Healths>,
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

/// Every failing observation: each link of a declared plugin and each mail account whose
/// results read as failing. A model that does not answer is the bootstrap's to ask for
/// (`bootstrap::owed`).
#[must_use]
pub fn failing(observed: &Observed<'_>) -> Vec<Failing> {
    let connections = observed.declared.connections();
    let of = |health: Option<&Health>| health.and_then(Health::remedial);
    let connected = connections.values().filter_map(|connection| {
        let reading = match connection.template().table {
            Table::Link(fragment) => {
                let name = connection.secrets().into_iter().next()?;
                observed
                    .links
                    .iter()
                    .find(|(plugin, _)| plugin.as_str() == fragment.plugin)
                    .and_then(|(_, links)| links.health(&name))
            }
            Table::Account(_) => observed.mail.health(&connection.key().name),
            Table::Always | Table::Model | Table::Folder | Table::Channel(_) => None,
        };
        let (class, since) = of(reading)?;
        Some(Failing {
            connection: connection.clone(),
            class,
            since,
        })
    });
    connected.collect()
}

/// The cards the connections' health owes, besides what `asked` already asks for: one per
/// failing connection and episode whose remedy a tap gives, a secret or a sign-in another rule
/// already asks for being that card; and each removal an unused connection is offered.
#[must_use]
pub fn owed(observed: &Observed<'_>, asked: &Owed) -> Owed {
    let settables = observed.answering.settables;
    let asks = |proposal: &Proposal| {
        asked
            .iter()
            .any(|noticed| match (&noticed.proposal, proposal) {
                (
                    Proposal::Act(Act::Secret { name: held, .. }),
                    Proposal::Act(Act::Secret { name, .. }),
                ) => held == name,
                (held, proposal) => held == proposal,
            })
    };
    let failing = failing(observed);
    let fixes = failing
        .iter()
        .filter_map(|failing| failing.fix(settables))
        .filter(|owed| !owed.iter().any(|noticed| asks(&noticed.proposal)))
        .fold(Owed::default(), Owed::join);
    // A failure a tap mends has its card, from this rule or another; removing the connection
    // beside it would offer the opposite act for the same state.
    let mended: BTreeSet<Key> = failing
        .iter()
        .filter(|failing| failing.class.remedy() != Remedy::Retry)
        .map(|failing| failing.connection.key())
        .collect();
    fixes.join(unused(observed, &mended))
}

/// The removals an unused connection is offered, none for a connection in `mended`.
fn unused(observed: &Observed<'_>, mended: &BTreeSet<Key>) -> Owed {
    let connections = observed.declared.connections();
    let accounts = connections
        .values()
        .filter(|connection| !mended.contains(&connection.key()))
        .filter_map(|connection| {
            match connection.template().table {
                Table::Account(_) => {}
                Table::Always
                | Table::Model
                | Table::Link(_)
                | Table::Folder
                | Table::Channel(_) => return None,
            }
            let reading = observed.mail.reading(&connection.key().name)?;
            let since = match reading {
                Reading::Failing { since, .. }
                    if reading.failing_for(observed.now) >= Some(abandoned()) =>
                {
                    since
                }
                Reading::Failing { .. } | Reading::Ok { .. } => return None,
            };
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
        Some(heard) => connections
            .values()
            .filter(|connection| !mended.contains(&connection.key()))
            .filter_map(|connection| quiet_calendar(observed, heard, connection))
            .fold(Owed::default(), Owed::join),
    };
    accounts.fold(Owed::default(), Owed::join).join(calendars)
}

/// The removal a calendar is offered when its link syncs, its latest fetch included, and its
/// store holds nothing within [`quiet`] of now: a calendar that holds nothing because its link
/// fails is offered none.
fn quiet_calendar(
    observed: &Observed<'_>,
    heard: &BTreeSet<CalendarName>,
    connection: &Connection,
) -> Option<Owed> {
    let fragment = match connection.template().table {
        Table::Link(fragment) => fragment,
        Table::Always | Table::Model | Table::Account(_) | Table::Folder | Table::Channel(_) => {
            return None;
        }
    };
    let name = connection.secrets().into_iter().next()?;
    let synced = observed
        .links
        .iter()
        .find(|(plugin, _)| plugin.as_str() == fragment.plugin)
        .and_then(|(_, links)| links.health(&name))
        .and_then(|health| match health.reading() {
            Reading::Ok { at } => Some(at),
            Reading::Failing { .. } => None,
        })?;
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
    use crate::mail::health::Outcome;
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

    fn ics() -> PluginName {
        PluginName::parse("ics").expect("a name")
    }

    /// A deployment declaring the Work calendar and Ada's Microsoft account.
    struct World {
        declared: Declared,
        settables: Settables,
        answered: Healths,
        mail: Healths,
        links: BTreeMap<PluginName, Healths>,
        heard: BTreeSet<CalendarName>,
    }

    impl World {
        fn new() -> Self {
            let declared = Declared::default()
                .with(&work())
                .and_then(|declared| declared.with(&account()))
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
                mail: Healths::default(),
                links: BTreeMap::new(),
                heard: BTreeSet::new(),
            }
        }

        fn link(&mut self, at: Instant, outcome: Outcome) {
            let joined = self
                .links
                .get(&ics())
                .cloned()
                .unwrap_or_default()
                .join(&Healths::one("PLUGIN_ICS_WORK", at, outcome, "it answered"));
            self.links.insert(ics(), joined);
        }

        fn owed(&self, now: Instant) -> Owed {
            let resolves = |_: &Settable| true;
            let answering = Answering {
                model: "a-cli/a-model",
                health: &self.answered,
                settables: &self.settables,
                resolves: &resolves,
            };
            owed(
                &Observed {
                    declared: &self.declared,
                    answering: &answering,
                    mail: &self.mail,
                    links: &self.links,
                    heard: Some(&self.heard),
                    now,
                },
                &Owed::default(),
            )
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

    fn outcome(result: Seen) -> Outcome {
        match result {
            Seen::Synced => Outcome::Succeeded,
            Seen::Refused(status) => Outcome::Failed(Failure::of_status(status)),
            Seen::Unreached => Outcome::Failed(Failure::Unreachable),
            Seen::Outage => Outcome::Failed(Failure::Unclassified),
        }
    }
}
