//! The failing sources' rule, over the stores held in core's memory.

use chrono::Duration;
use std::collections::BTreeSet;
use suzatary_cards::secretary::bootstrap::Answering;
use suzatary_cards::secretary::failing::*;
use suzatary_cards::secretary::noticed::Owed;
use suzatary_connect::connect::{Connection, Declared};
use suzatary_connect::settings::{Settable, Settables};
use suzatary_google::calendar::CalendarName;
use suzatary_health::health::Failure;
use suzatary_health::health::Outcome;
use suzatary_health::health::SourceKey;
use suzatary_health::health::{Healths, Instant};
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::inbox::Fold;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::Proposal;
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_stores::stores::InMemory;

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
                suzatary_connect::settings::MODEL_TOKEN,
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
    conversations: &suzatary_stores::stores::conversations::InMemory,
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
    /// However often each failure is seen, whatever its class moves to between sightings,
    /// and whatever the box restarts between filings, one card is filed per episode of a
    /// failing link, an episode being every result since one success; and the card stays as
    /// it stood until a success.
    #[test]
    fn one_act_per_connection_and_since(
        script in proptest::collection::vec((seen(), 1u8..4, proptest::bool::ANY), 1..20)
    ) {
        let mut world = World::new();
        let inbox_store = InMemory::<InboxEvent>::default();
        let conversations = suzatary_stores::stores::conversations::InMemory::default();
        let mut episodes = 0usize;
        let mut open_episode = false;
        let mut hour = 0;
        for (result, sightings, dismiss) in script {
            hour += 1;
            world.link(at(hour), outcome(result));
            match result {
                Seen::Synced => open_episode = false,
                Seen::Refused(_) | Seen::Unreached | Seen::Outage => {
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
                Some(suzatary_inbox::secretary::responded::Responded::Withdrawn(_)))
        }).count();
        // Only an episode a success ended withdraws its card.
        proptest::prop_assert!(withdrawn <= episodes.saturating_sub(usize::from(open_episode)));
    }
}

/// A Google account's calendars failing alone for a week leave its mail in use, so no removal
/// is offered; a sign-in its mail needs is owed its card whatever older outage its calendars
/// have; and a sign-in card another rule already asks for, for one missing part, is that card.
#[test]
fn a_google_accounts_two_sources() {
    let bea = suzatary_mail::mail::account_email::AccountEmail::new("bea@example.test")
        .expect("an address");
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
            scopes: suzatary_google::google::Scopes::of([suzatary_google::google::Scope::Calendar])
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

/// The secrets rule and this one ask for a link stored nowhere once between them, however
/// many times the box files and whatever class its failure moves to: one card waits, its
/// words name no earlier card, and a filing over an unchanged state writes nothing.
#[test]
fn a_missing_link_is_asked_for_by_one_card_however_often_the_box_files() {
    let mut world = World::new();
    let inbox_store = InMemory::<InboxEvent>::default();
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let missing = |name: &Settable| name.name().starts_with("PLUGIN_ICS");
    let failures = [
        Seen::Refused(401),
        Seen::Unreached,
        Seen::Outage,
        Seen::Refused(404),
    ];
    for hour in 1..=24 {
        world.link(at(hour), outcome(failures[hour as usize % failures.len()]));
        for _ in 0..3 {
            let fold = inbox::fold(&inbox_store).expect("reads");
            let owed = suzatary_cards::secretary::fixes::owed(&world.settables, missing, &fold)
                .join(world.owed(at(hour)));
            let before = suzatary_store::store::Reads::objects(&inbox_store)
                .expect("reads")
                .len();
            file(&owed, &inbox_store, &conversations, hour);
            let after = suzatary_store::store::Reads::objects(&inbox_store)
                .expect("reads")
                .len();
            if hour > 1 {
                assert_eq!(after, before, "hour {hour}: a filing wrote");
            }
        }
        let fold = inbox::fold(&inbox_store).expect("reads");
        assert_eq!(secrets_filed(&fold), 1, "hour {hour}");
        let waiting: Vec<_> = fold
            .objects()
            .filter(|object| object.reading.open())
            .filter_map(|object| match &object.asked.proposal {
                Proposal::Act(Act::Secret { why, .. }) => Some(why.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(waiting.len(), 1, "hour {hour}");
        assert!(!waiting[0].contains("proposal:"), "{}", waiting[0]);
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
