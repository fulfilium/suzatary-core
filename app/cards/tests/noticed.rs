//! The noticed secrets' rule, over the stores held in core's memory.

use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::{ProposalRef, Reference};
use suzatary_cards::secretary::noticed::owing::{Store, at, owed, secret};
use suzatary_cards::secretary::noticed::*;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::inbox::ProposalId;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::{Proposal, Reasons};
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_inbox::secretary::responded::Withdrawal;
use suzatary_lines::secretary::{conversations, transcript};
use suzatary_store::store::Reads as _;
use suzatary_stores::stores::InMemory;

/// Filing a set twice writes nothing the second time; a card the next set does not hold is
/// withdrawn while it waits, and one answered stays as it was answered.
#[test]
fn filing_is_idempotent_and_withdraws_what_is_no_longer_owed() {
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let first = owed(&["a", "b"])
        .file(conversations.conversations(), &inbox_store, at(0))
        .expect("filed");
    assert_eq!(first.owed.len(), 2);
    let objects = inbox_store.objects().expect("reads").len();
    let lines = conversations.lines.objects().expect("reads").len();
    let again = owed(&["a", "b"])
        .file(conversations.conversations(), &inbox_store, at(5))
        .expect("filed");
    assert_eq!(again, first, "the same set is the same cards");
    assert_eq!(inbox_store.objects().expect("reads").len(), objects);
    assert_eq!(conversations.lines.objects().expect("reads").len(), lines);

    let later = owed(&["a"])
        .file(conversations.conversations(), &inbox_store, at(9))
        .expect("filed");
    assert_eq!(later.withdrawn, vec![first.owed[1].clone()]);
    let fold = inbox::fold(&inbox_store).expect("reads");
    assert!(matches!(
        fold.get(&first.owed[1])
            .expect("held")
            .reading
            .shown()
            .map(|answer| &answer.response),
        Some(suzatary_inbox::secretary::responded::Responded::Withdrawn(
            Withdrawal::NoLongerNeeded { why }
        )) if why == WITHDRAWN
    ));
    assert!(fold.get(&first.owed[0]).expect("held").reading.open());
    let quiet = owed(&["a"])
        .file(conversations.conversations(), &inbox_store, at(10))
        .expect("filed");
    assert!(
        quiet.withdrawn.is_empty(),
        "a withdrawn card is withdrawn once"
    );
}

/// Only the box's own cards are its to withdraw.
#[test]
fn a_card_someone_else_asked_for_is_never_withdrawn() {
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let asked = inbox::Asked {
        proposer: Proposer::Secretary,
        proposal: secret("THEIRS"),
        reasons: Reasons::new(
            "asked in a round",
            vec!["briefing:2026-10-07".parse().expect("a reference")],
        )
        .expect("reasons"),
        asked: ProposalRef {
            conversation: ConversationId::new("default").expect("a slug"),
            index: 0,
        },
        price: inbox::Price::unpriced(),
    };
    let theirs = inbox::ask(&inbox_store, asked, &transcript::writer(), at(0)).expect("filed");
    let filed = Owed::default()
        .file(conversations.conversations(), &inbox_store, at(1))
        .expect("filed");
    assert!(filed.withdrawn.is_empty());
    let fold = inbox::fold(&inbox_store).expect("reads");
    assert!(fold.get(&theirs).expect("held").reading.open());
}

/// The box files cards outside the setup conversation, a build's and an erasure's, and the
/// setup set withdraws none of them.
#[test]
fn the_box_s_cards_elsewhere_are_not_the_setup_set_s() {
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let boxs = |conversation: &str, proposal: Proposal| {
        let asked = inbox::Asked {
            proposer: Proposer::Box,
            proposal,
            reasons: Reasons::new(
                "the box asks",
                vec!["briefing:2026-10-07".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: ProposalRef {
                conversation: ConversationId::new(conversation).expect("a slug"),
                index: 0,
            },
            price: inbox::Price::unpriced(),
        };
        inbox::ask(&inbox_store, asked, &transcript::writer(), at(0)).expect("filed")
    };
    let build = boxs(
        "build",
        Proposal::Act(suzatary_inbox::secretary::acts::Act::CodingSession {
            task: suzatary_inbox::coding::Task::try_from("Build todo:suzatary/x.".to_owned())
                .expect("a task"),
        }),
    );
    let erase = boxs(
        "default",
        Proposal::Act(suzatary_inbox::secretary::acts::Act::Erase {
            fact: suzatary_memory::secretary::memory::FactId(2),
            keys: [
                suzatary_memory::secretary::locked_text::KeyName::new("a".repeat(64))
                    .expect("a key name"),
            ]
            .into(),
        }),
    );
    let filed = Owed::default()
        .file(conversations.conversations(), &inbox_store, at(1))
        .expect("filed");
    assert!(filed.withdrawn.is_empty(), "{:?}", filed.withdrawn);
    let fold = inbox::fold(&inbox_store).expect("reads");
    for card in [build, erase] {
        assert!(fold.get(&card).expect("held").reading.open());
    }
}

fn signing_in() -> Proposal {
    Proposal::Act(suzatary_inbox::secretary::acts::Act::Auth {
        account: suzatary_mail::mail::address::EmailAddress::new("ada@example.com")
            .expect("a shaped address"),
    })
}

/// The secretary's own card for `proposal`, asked at `minute`.
fn secretarys(inbox_store: &InMemory<InboxEvent>, proposal: Proposal, minute: u32) -> ProposalId {
    let asked = inbox::Asked {
        proposer: Proposer::Secretary,
        proposal,
        reasons: Reasons::new(
            "asked in a round",
            vec!["briefing:2026-10-07".parse().expect("a reference")],
        )
        .expect("reasons"),
        asked: ProposalRef {
            conversation: ConversationId::new("default").expect("a slug"),
            index: 0,
        },
        price: inbox::Price::unpriced(),
    };
    inbox::ask(inbox_store, asked, &transcript::writer(), at(minute)).expect("filed")
}

/// The Setup conversation's turns, none before it is opened.
fn said(conversations: &suzatary_stores::stores::conversations::InMemory) -> usize {
    conversations::read(
        conversations.conversations(),
        &suzatary_base::conversation_id::SETUP,
    )
    .map_or(0, |stored| stored.live().len())
}

/// While the secretary's sign-in card waits, the box's owed sign-in is that card: filing the
/// same set again says nothing in Setup and files nothing.
#[test]
fn a_waiting_card_of_the_secretary_s_holds_the_box_s_and_says_nothing() {
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let theirs = secretarys(&inbox_store, signing_in(), 1);
    let owed = Owed::one(signing_in(), "the account was connected").since(at(0));
    let before = said(&conversations);
    for minute in [2, 3] {
        let filed = owed
            .file(conversations.conversations(), &inbox_store, at(minute))
            .expect("filed");
        assert_eq!(filed.owed, vec![theirs.clone()]);
    }
    assert_eq!(said(&conversations), before);
    assert_eq!(
        inbox::fold(&inbox_store).expect("reads").objects().count(),
        1
    );
}

/// The secretary's card dismissed within the episode answers the box's asking for it; once
/// a new episode begins, the box asks again.
#[test]
fn a_card_dismissed_within_the_episode_holds_it() {
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let theirs = secretarys(&inbox_store, signing_in(), 3);
    inbox::dismiss(
        &inbox_store,
        &theirs,
        &transcript::writer(),
        at(4),
        "not now",
    )
    .expect("dismissed");
    let within = Owed::one(signing_in(), "the account was connected")
        .since(at(1))
        .file(conversations.conversations(), &inbox_store, at(5))
        .expect("filed");
    assert_eq!(within.owed, vec![theirs.clone()]);
    let next = Owed::one(signing_in(), "the account was connected again")
        .since(at(6))
        .file(conversations.conversations(), &inbox_store, at(7))
        .expect("filed");
    assert_ne!(next.owed, vec![theirs]);
    assert_eq!(next.owed.len(), 1);
}

/// The box's open cards, as the acts they ask for.
fn open(inbox_store: &InMemory<InboxEvent>) -> Vec<suzatary_inbox::secretary::acts::Act> {
    inbox::fold(inbox_store)
        .expect("reads")
        .objects()
        .filter(|object| object.asked.proposer == Proposer::Box && object.reading.open())
        .filter_map(|object| match &object.asked.proposal {
            Proposal::Act(act) => Some(act.clone()),
            _ => None,
        })
        .collect()
}

fn noticed(
    store: &Store,
    conversations: &suzatary_stores::stores::conversations::InMemory,
    inbox_store: &InMemory<InboxEvent>,
    minute: u32,
) -> Filed {
    let settables = suzatary_connect::settings::Settables::derive(
        suzatary_connect::settings::MODEL_TOKEN,
        suzatary_connect::connect::Channel::ALL,
        None,
        &[],
    );
    let seen = Seen {
        settables: &settables,
        secrets: store,
        model: "a-cli/large",
        answered: &suzatary_health::health::Healths::default(),
        open: &[],
        declared: Some(&suzatary_connect::connect::Declared::default()),
        health: Some(&suzatary_health::health::Healths::default()),
        heard: Some(&std::collections::BTreeSet::new()),
        pushed: &suzatary_connect::settings::remote::Pushed::Nowhere,
        now: prodrome_core::fpl::instant_of(at(minute)),
        redirect: None,
    };
    notice(
        &seen,
        conversations.conversations(),
        inbox_store,
        at(minute),
    )
    .expect("filed")
}

/// A locked store neither asks for a key it may hold nor withdraws a card still waiting: the
/// card stays, and the one card the box adds asks to unlock the store; once it answers, that
/// card is withdrawn and the waiting one is as it was.
#[test]
fn a_locked_store_holds_every_card_and_asks_once_to_be_unlocked() {
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let answering = Store {
        locked: Vec::new(),
        failed: Vec::new(),
        ..Store::default()
    };
    noticed(&answering, &conversations, &inbox_store, 0);
    let waiting = open(&inbox_store);
    assert!(
        waiting.iter().any(|act| matches!(act,
            suzatary_inbox::secretary::acts::Act::Secret { name, .. } if name == "NTFY_TOPIC")),
        "{waiting:?}"
    );

    let locked = Store {
        locked: vec!["NTFY_TOPIC", suzatary_connect::settings::MODEL_TOKEN],
        failed: Vec::new(),
        ..Store::default()
    };
    let filed = noticed(&locked, &conversations, &inbox_store, 1);
    assert!(filed.withdrawn.is_empty(), "{filed:?}");
    let held = open(&inbox_store);
    for act in &waiting {
        assert!(
            held.contains(act),
            "{act:?} stays while the store is locked"
        );
    }
    let unlocks: Vec<_> = held
        .iter()
        .filter(|act| {
            matches!(
                act,
                suzatary_inbox::secretary::acts::Act::UnlockStore { .. }
            )
        })
        .collect();
    assert_eq!(unlocks.len(), 1, "{held:?}");
    assert_eq!(
        held.len(),
        waiting.len() + 1,
        "nothing else is filed: {held:?}"
    );
    let again = noticed(&locked, &conversations, &inbox_store, 2);
    assert!(again.withdrawn.is_empty());
    assert_eq!(open(&inbox_store), held, "filing again files nothing");

    let unlocked = noticed(&answering, &conversations, &inbox_store, 3);
    assert_eq!(unlocked.withdrawn.len(), 1, "the unlock card, and only it");
    assert_eq!(open(&inbox_store), waiting);
}

/// A name the store failed on is that name's: it is asked for by no card and holds the
/// waiting ones, while every missing name is still asked for, and no unlock is asked that
/// would mend nothing.
#[test]
fn a_name_the_store_failed_on_costs_only_itself() {
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let broken = Store {
        locked: Vec::new(),
        failed: vec!["NTFY_TOPIC"],
        ..Store::default()
    };
    let filed = noticed(&broken, &conversations, &inbox_store, 0);
    assert!(filed.withdrawn.is_empty(), "{filed:?}");
    let cards = open(&inbox_store);
    let asks = |wanted: &str| {
        cards.iter().any(|act| {
            matches!(act,
            suzatary_inbox::secretary::acts::Act::Secret { name, .. } if name == wanted)
        })
    };
    assert!(!asks("NTFY_TOPIC"), "{cards:?}");
    assert!(
        asks("FCM_SERVICE_ACCOUNT_B64"),
        "a missing name is still asked for: {cards:?}"
    );
    assert!(
        !cards.iter().any(|act| matches!(
            act,
            suzatary_inbox::secretary::acts::Act::UnlockStore { .. }
        )),
        "{cards:?}"
    );
    let answering = Store::default();
    noticed(&answering, &conversations, &inbox_store, 1);
    let waiting = open(&inbox_store);
    let refiled = noticed(&broken, &conversations, &inbox_store, 2);
    assert!(
        refiled.withdrawn.is_empty(),
        "a failed name holds its card: {refiled:?}"
    );
    assert_eq!(open(&inbox_store), waiting);
}

/// A calendar whose link fails is asked for its link once, whichever rule asks: by the
/// failing-source rule while its secret is stored, and by the secrets rule once it is lost.
/// The secret's loss is the same need, so no notice after the first appends anything, and
/// one card for the link stands.
#[test]
fn a_failing_link_whose_secret_is_lost_is_asked_for_once() {
    let work = suzatary_connect::connect::Connection::parse("ics", &["Work".to_owned()])
        .expect("a calendar");
    let declared = suzatary_connect::connect::Declared::default()
        .with(&work)
        .expect("declared");
    let settables = suzatary_connect::settings::Settables::derive(
        suzatary_connect::settings::MODEL_TOKEN,
        [],
        None,
        &declared.plugins,
    );
    let link = work.secrets().into_iter().next().expect("a link's secret");
    let health = suzatary_health::health::Healths::one(
        work.sources().into_iter().next().expect("a link's source"),
        prodrome_core::fpl::instant_of(at(0)),
        suzatary_health::health::Outcome::Failed(suzatary_health::health::Failure::of_status(401)),
        "it answered 401",
    );
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let notice_at = |store: &Store, minute: u32| {
        let seen = Seen {
            settables: &settables,
            secrets: store,
            model: "a-cli/large",
            answered: &suzatary_health::health::Healths::default(),
            open: &[],
            declared: Some(&declared),
            health: Some(&health),
            heard: Some(&std::collections::BTreeSet::new()),
            pushed: &suzatary_connect::settings::remote::Pushed::Nowhere,
            now: prodrome_core::fpl::instant_of(at(minute)),
            redirect: None,
        };
        notice(
            &seen,
            conversations.conversations(),
            &inbox_store,
            at(minute),
        )
        .expect("filed")
    };
    let written = || {
        (
            inbox_store.objects().expect("reads").len(),
            conversations.lines.objects().expect("reads").len(),
        )
    };
    let asking = || {
        open(&inbox_store)
            .into_iter()
            .filter(|act| {
                matches!(act, suzatary_inbox::secretary::acts::Act::Secret { name, .. } if *name == link)
            })
            .count()
    };
    let stored = Store {
        set: vec![link.clone()],
        ..Store::default()
    };
    notice_at(&stored, 1);
    assert_eq!(asking(), 1, "the failing link is asked for its link");
    let settled = written();
    let lost = Store::default();
    for minute in 2..6 {
        let filed = notice_at(&lost, minute);
        assert!(filed.withdrawn.is_empty(), "{minute}: {filed:?}");
        assert_eq!(
            written(),
            settled,
            "a notice over the same state appends nothing"
        );
    }
    assert_eq!(asking(), 1, "one card asks for the link");
}

/// A link a Google account's calendar was pasted as is asked for until that account reads its
/// calendars; then its removal is asked for instead, and its link no longer is, while a link
/// nothing tied to the account is still asked for.
#[test]
fn a_link_pasted_as_a_google_calendar_is_retired_once_the_account_reads_its_calendars() {
    use suzatary_inbox::secretary::acts::Act;
    let calendar = |label: &str| {
        suzatary_connect::connect::Connection::parse("ics", &[label.to_owned()])
            .expect("a calendar")
    };
    let (work, home) = (calendar("Work"), calendar("Home"));
    let ada =
        suzatary_connect::connect::Connection::parse("google", &["ada@example.test".to_owned()])
            .expect("a Google account");
    let declared = [&work, &home, &ada]
        .into_iter()
        .try_fold(
            suzatary_connect::connect::Declared::default(),
            |declared, connection| declared.with(connection),
        )
        .expect("declared");
    let settables = suzatary_connect::settings::Settables::derive(
        suzatary_connect::settings::MODEL_TOKEN,
        [],
        None,
        &declared.plugins,
    );
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let notice_at = |store: &Store, minute: u32| {
        let seen = Seen {
            settables: &settables,
            secrets: store,
            model: "a-cli/large",
            answered: &suzatary_health::health::Healths::default(),
            open: &[],
            declared: Some(&declared),
            health: Some(&suzatary_health::health::Healths::default()),
            heard: Some(&std::collections::BTreeSet::new()),
            pushed: &suzatary_connect::settings::remote::Pushed::Nowhere,
            now: prodrome_core::fpl::instant_of(at(minute)),
            redirect: None,
        };
        notice(
            &seen,
            conversations.conversations(),
            &inbox_store,
            at(minute),
        )
        .expect("filed")
    };
    let link = |connection: &suzatary_connect::connect::Connection| connection.secrets().remove(0);
    let asks = |wanted: &str| {
        open(&inbox_store)
            .iter()
            .any(|act| matches!(act, Act::Secret { name, .. } if name == wanted))
    };
    let removes = |link: &suzatary_connect::connect::Connection| {
        open(&inbox_store)
            .iter()
            .any(|act| matches!(act, Act::Disconnect { connection } if connection == link))
    };
    let unsigned = Store::default();
    notice_at(&unsigned, 0);
    assert!(asks(&link(&work)) && asks(&link(&home)));
    let pasted = inbox::fold(&inbox_store)
        .expect("reads")
        .objects()
        .find(|object| {
            matches!(&object.asked.proposal, Proposal::Act(Act::Secret { name, .. }) if *name == link(&work))
        })
        .map(|object| Reference::Proposal(object.asked.asked.clone()))
        .expect("the Work calendar's card");
    // What a tap of that card with a link to one of ada's calendars files.
    Owed::one(Proposal::Act(Act::Connect { connection: ada }), "pasted")
        .about(&[pasted])
        .since(at(1))
        .holding()
        .file(conversations.conversations(), &inbox_store, at(1))
        .expect("filed");
    notice_at(&unsigned, 2);
    assert!(
        asks(&link(&work)),
        "the account does not read its calendars yet"
    );
    assert!(!removes(&work));

    let signed = Store {
        granted: Some(suzatary_google::google::Scopes::all()),
        ..Store::default()
    };
    notice_at(&signed, 3);
    assert!(!asks(&link(&work)), "its link is no longer asked for");
    assert!(removes(&work), "its removal is asked for");
    assert!(
        asks(&link(&home)) && !removes(&home),
        "the other calendar is still asked for"
    );
    let settled = open(&inbox_store);
    notice_at(&signed, 4);
    assert_eq!(open(&inbox_store), settled, "filing again changes nothing");
}

/// A locked answer is the store's: the box asks once and files the unlock card, and never
/// waits out the store's timeout for each of the other names; a store that answers is asked
/// once per name.
#[test]
fn a_locked_store_is_asked_once_and_an_answering_one_once_per_name() {
    let conversations = suzatary_stores::stores::conversations::InMemory::default();
    let inbox_store = InMemory::<InboxEvent>::default();
    let every = [
        suzatary_connect::settings::MODEL_TOKEN,
        "FCM_SERVICE_ACCOUNT_B64",
        "NTFY_TOPIC",
    ];
    let locked = Store {
        locked: every.to_vec(),
        ..Store::default()
    };
    noticed(&locked, &conversations, &inbox_store, 0);
    assert_eq!(locked.asked.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(open(&inbox_store).iter().any(|act| matches!(
        act,
        suzatary_inbox::secretary::acts::Act::UnlockStore { .. }
    )));
    let answering = Store::default();
    noticed(&answering, &conversations, &inbox_store, 1);
    assert_eq!(
        answering.asked.load(std::sync::atomic::Ordering::SeqCst),
        every.len()
    );
}
