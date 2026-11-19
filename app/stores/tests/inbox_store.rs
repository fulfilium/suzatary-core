//! The inbox's fold over a store that holds its proposals: a proposal named by its content, and
//! answers given apart that settle or conflict. Over core's in-memory store.

use prodrome_core::dag::Dag;
use prodrome_core::event::{Actor, Hash};
use prodrome_core::literal::Datetime;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::card;
use suzatary_inbox::secretary::inbox::*;
use suzatary_inbox::secretary::inbox_event::{InboxEvent, Said};
use suzatary_inbox::secretary::inbox_reading::Reading;
use suzatary_inbox::secretary::proposal::{Draft, Proposal, Reasons};
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_inbox::secretary::responded::{Done, Responded, Withdrawal};
use suzatary_store::store::{Prodrome as _, Reads as _};
use suzatary_stores::stores::InMemory;

fn draft(subject: &str) -> Proposal {
    Proposal::Email(Draft {
        account: "ada@example.com".to_owned(),
        to: vec!["mira@example.com".to_owned()],
        cc: Vec::new(),
        subject: subject.to_owned(),
        body: "Thursday works.".to_owned(),
        attachments: Vec::new(),
    })
}

fn asked(subject: &str) -> Asked {
    Asked {
        proposer: Proposer::Secretary,
        proposal: draft(subject),
        reasons: Reasons::new(
            "she asked for a day",
            vec!["mail:abcdef012345".parse().expect("a ref")],
        )
        .expect("reasons"),
        asked: "proposal:default/0".parse().expect("a ref"),
        price: Price::Serves("todo:life/reply".parse().expect("a ref")),
    }
}

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 9, 28, 9, minute, 0, 0).expect("an instant")
}

fn signing_in(asking: &str) -> Asked {
    Asked {
        proposer: Proposer::Secretary,
        proposal: Proposal::Act(Act::Auth {
            account: suzatary_mail::mail::address::EmailAddress::new("ada@example.com")
                .expect("a shaped address"),
        }),
        reasons: Reasons::new(asking, vec!["mail:abcdef012345".parse().expect("a ref")])
            .expect("reasons"),
        asked: "proposal:default/1".parse().expect("a ref"),
        price: Price::unpriced(),
    }
}

/// A proposal filed under a name an earlier print gave it is found by its content: asking
/// the same act again answers that name and writes nothing, and a recurring act asked at
/// another instant is another proposal.
#[test]
fn the_same_content_is_the_same_proposal_whatever_named_it() {
    let store = InMemory::<InboxEvent>::default();
    let ada = Actor::new("ada").expect("an actor");
    let carried = ProposalId::new("p0123456789abcdef0123456789abcdef").expect("an id");
    store
        .append(InboxEvent {
            proposal: carried.clone(),
            at: at(0),
            actor: ada.clone(),
            said: Said::Asked(Box::new(asked("a"))),
        })
        .expect("files under the old name");
    let before = store.objects().expect("reads").len();
    assert_eq!(ask(&store, asked("a"), &ada, at(5)), Ok(carried.clone()));
    assert_eq!(
        store.objects().expect("reads").len(),
        before,
        "nothing written"
    );
    dismiss(&store, &carried, &ada, at(6), "not this week").expect("dismissed");
    assert_eq!(
        ask(&store, asked("a"), &ada, at(7)),
        Ok(carried.clone()),
        "a settled proposal is the answer too"
    );
    let first = ask(&store, signing_in("sign in"), &ada, at(8)).expect("asks");
    assert_eq!(
        ask(&store, signing_in("sign in"), &ada, at(8)),
        Ok(first.clone())
    );
    assert_ne!(ask(&store, signing_in("sign in"), &ada, at(9)), Ok(first));
}

proptest::proptest! {
    /// Over any history the inbox's own `ask` wrote, the reading names every proposal by
    /// its content: `holding` is `get` of the content's id, every asking is held, and a
    /// content never asked is held by nothing.
    #[test]
    fn holding_is_get_of_the_content_id(
        asks in proptest::collection::vec((0u8..3, proptest::bool::ANY, 0u32..3), 1..10),
    ) {
        let store = InMemory::<InboxEvent>::default();
        let ada = Actor::new("ada").expect("an actor");
        let make = |subject: u8, recurring: bool| {
            if recurring {
                let mut asked = signing_in("sign in again");
                asked.asked = format!("proposal:default/{subject}").parse().expect("a ref");
                asked
            } else {
                asked(&format!("subject {subject}"))
            }
        };
        for (subject, recurring, minute) in &asks {
            ask(&store, make(*subject, *recurring), &ada, at(*minute)).expect("asks");
        }
        let fold = fold(&store).expect("reads");
        for subject in 0..4 {
            for recurring in [false, true] {
                for minute in 0..4 {
                    let asked = make(subject, recurring);
                    let held = fold.holding(&asked, at(minute));
                    proptest::prop_assert_eq!(
                        held,
                        fold.get(&ProposalId::of(&asked, at(minute)))
                    );
                    let was_asked = asks.iter().any(|(s, r, m)| {
                        *r == recurring
                            && (if recurring { *m == minute } else { *s == subject })
                    });
                    proptest::prop_assert_eq!(held.is_some(), was_asked);
                }
            }
        }
    }
}

/// The box's asking of a sign-in, an episode, is held by the secretary's earlier recurring
/// asking of the same act while that card still waits: one request, one card.
#[test]
fn the_box_asking_an_act_the_secretary_asked_is_the_same_proposal() {
    let store = InMemory::<InboxEvent>::default();
    let ada = Actor::new("ada").expect("an actor");
    let first = ask(&store, signing_in("sign in"), &ada, at(1)).expect("asks");
    let boxs = Asked {
        proposer: Proposer::Box,
        ..signing_in("the box noticed")
    };
    assert_eq!(ask(&store, boxs, &ada, at(5)), Ok(first));
}

/// A card of the act answered no holds nothing, so the box's next episode is a new card;
/// one still waiting holds it.
#[test]
fn a_dismissed_card_does_not_hold_the_box_s_next_episode() {
    let store = InMemory::<InboxEvent>::default();
    let ada = Actor::new("ada").expect("an actor");
    let boxs = || Asked {
        proposer: Proposer::Box,
        ..signing_in("the box noticed")
    };
    let first = ask(&store, signing_in("sign in"), &ada, at(1)).expect("asks");
    dismiss(&store, &first, &ada, at(2), "not now").expect("dismisses");
    let again = ask(&store, boxs(), &ada, at(5)).expect("asks again");
    assert_ne!(again, first);
    assert_eq!(ask(&store, boxs(), &ada, at(7)), Ok(again));
}

/// Sent on one device and dismissed on another reads as a conflict of exactly those two
/// answers, least named first: it performs nothing, its card offers no answer, and a write is
/// refused until one response settles it.
#[test]
fn two_answers_apart_are_a_conflict_of_two() {
    let ada = Actor::new("ada").expect("an actor");
    let (one, two) = (
        InMemory::<InboxEvent>::default(),
        InMemory::<InboxEvent>::default(),
    );
    let id = ask(&one, asked("a"), &ada, at(0)).expect("asks");
    let asking: Vec<(Hash, String)> = one
        .objects()
        .expect("reads")
        .iter()
        .map(|(name, object)| {
            (
                name.clone(),
                prodrome_core::event::canonical_envelope(object),
            )
        })
        .collect();
    two.accept(&asking).expect("takes the asking");
    submit(&one, &id, &ada, at(1), |_| {
        Ok(Done::Sent {
            recipients: vec!["mira@example.com".to_owned()],
            message_id: "<1@box>".to_owned(),
        }
        .into())
    })
    .expect("sent here");
    dismiss(&two, &id, &ada, at(1), "not this one").expect("dismissed there");
    let union: Vec<_> = one
        .objects()
        .expect("reads")
        .into_iter()
        .chain(two.objects().expect("reads"))
        .collect();
    let fold = Fold::of(&Dag::from_iter(union).nodes().expect("one history"));
    let object = fold.get(&id).expect("asked");
    let Reading::Conflict(conflict) = &object.reading else {
        panic!("not a conflict: {}", object.reading);
    };
    assert_eq!(conflict.sides(), 2);
    assert!(
        conflict.first.name < conflict.second.name,
        "least named first"
    );
    assert_eq!(object.reading.shown(), Some(&conflict.first));
    assert!(!object.reading.performs(), "a conflict performs nothing");
    let card = card::of(&suzatary_inbox::wire::inbox::ProposedAct::Auth {
        account: "ada@example.com".to_owned(),
        sentence: "Sign in".to_owned(),
    });
    assert!(!card.answers.is_empty());
    assert!(
        card::at(card, &object.reading, &object.id)
            .answers
            .is_empty()
    );
    assert!(fold.one(&id).is_err());
}

/// Withdrawn on one replica and dismissed on another settles as the owner's no, with his
/// words, whichever write names the lesser object: a withdrawal is below a no.
#[test]
fn a_withdrawal_and_a_no_apart_settle_as_the_no() {
    let ada = Actor::new("ada").expect("an actor");
    let assistant = Actor::new("assistant").expect("an actor");
    let (one, two) = (
        InMemory::<InboxEvent>::default(),
        InMemory::<InboxEvent>::default(),
    );
    let id = ask(&one, asked("a"), &assistant, at(0)).expect("asks");
    let asking: Vec<(Hash, String)> = one
        .objects()
        .expect("reads")
        .iter()
        .map(|(name, object)| {
            (
                name.clone(),
                prodrome_core::event::canonical_envelope(object),
            )
        })
        .collect();
    two.accept(&asking).expect("takes the asking");
    withdraw(&one, &id, &assistant, at(1), Withdrawal::Superseded).expect("withdrawn here");
    dismiss(&two, &id, &ada, at(1), "not this one").expect("dismissed there");
    let union: Vec<_> = one
        .objects()
        .expect("reads")
        .into_iter()
        .chain(two.objects().expect("reads"))
        .collect();
    let fold = Fold::of(&Dag::from_iter(union).nodes().expect("one history"));
    let object = fold.get(&id).expect("asked");
    let Reading::One(answer) = &object.reading else {
        panic!("not settled: {}", object.reading);
    };
    assert_eq!(
        answer.response,
        Responded::Dismissed {
            reason: "not this one".to_owned()
        }
    );
    assert!(fold.one(&id).is_ok(), "nothing is left to settle");
}
