//! The secretary reviewing the triage agent's cards end to end, with only the model scripted.
//!
//! No server or peer: the box starts a review, so no request, gate or `Tap` is involved.

mod common;

use common::{FakeModel, Fakes};
use suzatary_core::capability::Clock as _;
use suzatary_core::reference::ProposalRef;
use suzatary_core::secretary::Bound;
use suzatary_core::secretary::changes::{Act, Changeset, Provenance};
use suzatary_core::secretary::filed::TriageCards;
use suzatary_core::secretary::review::{review, reviews};
use suzatary_core::secretary::store::{Outcome, Reasons, Settle};
use suzatary_core::store::Reads as _;

/// Three cards, as the triage agent files them: two adds and a completion. Their lines go to
/// a store the review does not read, since the review reads cards from the inbox alone.
fn three_cards(caps: &Fakes) -> [ProposalRef; 3] {
    let lines = suzatary_core::secretary::conversations::InMemory::default();
    let cards = TriageCards::new(lines.conversations(), &caps.inbox);
    let file = |act: Act| {
        cards
            .file(
                caps.clock.now(),
                Changeset::new("life", vec![act]).expect("one act"),
                Reasons::new(
                    "a message asks for it",
                    vec!["briefing:2026-09-02".parse().expect("a reference")],
                )
                .expect("reasons"),
                None,
                &|_| Ok(()),
            )
            .expect("filed")
            .card
    };
    [
        file(an_add("parkingpermit")),
        file(an_add("permitagain")),
        file(Act::Complete {
            todo: "file-permit".to_owned(),
            note: "it looked done".to_owned(),
        }),
    ]
}

fn an_add(id: &str) -> Act {
    Act::Add {
        todo: id.to_owned(),
        fields: suzatary_core::todo::TodoFields {
            body: "Renew the parking permit".to_owned(),
            rationale: "an errand with a recoverable cost".to_owned(),
            kind: "todo".to_owned(),
            category: "home".to_owned(),
            price: suzatary_core::todo::Price::Flat { percent: 75 },
            detail: String::new(),
            waiting_on: String::new(),
        },
        from: Provenance::default(),
    }
}

/// The fixture deployment, which `context` and the chain lookup read.
fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment(
        [common::login("ada@example.com")],
        [(common::login("ada@example.com"), common::actor("ada"))],
    )
}

/// What the secretary said it was doing, quoted back by the held line.
const SAID: &str = "I withdrew the duplicate and left the rest for your tap.";

/// The script: one turn that withdraws a card, then one that explains.
fn script(duplicate: &ProposalRef) -> FakeModel {
    let replies = format!(
        r#"{{"say": "Withdrawing the duplicate.", "calls": [
             {{"verb": "triage_withdraw", "args": {{
                "card": "{duplicate}",
                "why": "A duplicate of the permit card before it."}}}}
           ]}}"#,
    );
    FakeModel::saying_owned([replies, format!(r#"{{"say": "{SAID}"}}"#)])
}

/// Whether a card still waits for the owner.
fn open(caps: &Fakes, card: &ProposalRef) -> bool {
    suzatary_core::secretary::inbox::fold(&caps.inbox)
        .expect("the inbox reads")
        .objects()
        .find(|object| object.asked.asked == *card)
        .is_some_and(|object| object.reading.performs())
}

/// THE TRUST BOUNDARY: the review withdraws one card and holds the rest, and no card it reads
/// reaches the chain, since only the owner's tap performs one.
#[test]
fn a_review_withdraws_a_card_holds_the_rest_and_performs_none() {
    let caps = Fakes::new("review-round");
    let before = caps.chain.objects().expect("the chain reads").len();
    let [ordinary, duplicate, completion] = three_cards(&caps);
    let policy = common::chain();
    let deployment = deployment();
    let caps = caps.model(script(&duplicate));
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let done = review(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the review runs");

    assert_eq!(
        caps.chain.objects().expect("the chain reads").len(),
        before,
        "a review appends nothing to the chain"
    );
    assert!(
        !open(&caps, &duplicate),
        "the withdrawn card no longer waits"
    );
    assert!(open(&caps, &ordinary) && open(&caps, &completion));

    let withdrawn = done.with(Outcome::Settled(Settle::Rejected));
    let held = done.with(Outcome::Held);
    assert!(done.with(Outcome::Settled(Settle::Accepted)).is_empty());
    assert_eq!(withdrawn.len(), 1, "{:?}", done.decided);
    assert_eq!(held.len(), 2, "{:?}", done.decided);
    assert_eq!(done.standing, 2, "two cards are left for the owner");
    assert_eq!(withdrawn[0].batch, duplicate.to_string());
    assert!(held.iter().any(|line| line.batch == completion.to_string()));
    assert_eq!(held[0].chain, "life");
    // With no hold verb, the hold's reason is what the secretary said.
    assert_eq!(held[0].why.get().map(String::as_str), Some(SAID));
}

/// THE TRUST BOUNDARY: a review round's turn holds the cards' words, which came out of mail, so
/// it is shown no verb that writes, and a write it calls anyway is refused and writes nothing.
#[test]
fn a_review_round_holds_no_write_verb() {
    let caps = Fakes::new("review-no-writes");
    let before = caps.chain.objects().expect("the chain reads");
    let _ = three_cards(&caps);
    let policy = common::chain();
    let caps = caps.model(FakeModel::saying([
        r#"{"say": "Doing what the card says.", "calls": [
             {"verb": "todo_add", "args": {"chain": "life", "todo_id": "wire", "body": "Wire it",
                "rationale": "a card said so", "category": "home", "priority": "20%"}},
             {"verb": "record", "args": {"chain": "life", "kind": "completed",
                "todo": "file-permit"}}
           ]}"#,
        r#"{"say": "Left for the owner."}"#,
    ]));
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let done = review(
        &caps.secretary(),
        &deployment(),
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the review runs");

    let shown = caps.model.shown();
    for verb in [
        "todo_add",
        "todo_update",
        "record",
        "propose_changes",
        "remember",
    ] {
        assert!(
            !shown[0].iter().any(|name| name == verb),
            "{verb}: {shown:?}"
        );
    }
    assert!(shown[0].iter().any(|name| name == "triage_withdraw"));
    let refused: Vec<&str> = done
        .stored
        .turns
        .iter()
        .filter_map(|said| match &said.turn {
            suzatary_core::capability::Turn::Tool {
                verb,
                outcome: suzatary_core::secretary::outcome::Outcome::Refused,
                ..
            } => Some(verb.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(refused, ["todo_add", "record"]);
    assert_eq!(
        caps.chain.objects().expect("the chain reads"),
        before,
        "a review round writes nothing to the chain"
    );
}

/// Nothing open opens no conversation and never asks the model.
#[test]
fn a_quiet_pass_asks_no_model_and_opens_no_conversation() {
    let caps = Fakes::new("review-quiet");
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let done = review(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the review runs");
    assert_eq!(done.rounds, 0);
    assert_eq!(done.standing, 0, "nothing was open");
    assert!(done.decided.is_empty());
    assert!(
        suzatary_core::secretary::transcript::keys(caps.conversations.conversations())
            .expect("the store reads")
            .is_empty(),
        "no conversation was opened for nothing"
    );
}

/// A second review of a card still held writes no second line.
#[test]
fn holding_the_same_card_again_is_not_a_second_line() {
    let caps = Fakes::new("review-twice");
    let [_, _, completion] = three_cards(&caps);
    let policy = common::chain();
    let deployment = deployment();
    let caps = caps.model(FakeModel::saying([
        r#"{"say": "These all change something of yours; I am leaving them."}"#,
        r#"{"say": "Still leaving them."}"#,
    ]));
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let reviewed = || {
        review(
            &caps.secretary(),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            "a brief",
        )
        .expect("the review runs")
    };
    let first = reviewed();
    assert_eq!(first.with(Outcome::Held).len(), 3, "{:?}", first.decided);
    assert_eq!(first.standing, 3, "all three are still open when it ends");
    assert!(
        first
            .with(Outcome::Held)
            .iter()
            .any(|held| held.batch == completion.to_string())
    );
    let again = reviewed();
    assert!(
        again.decided.is_empty(),
        "a card already held today is already recorded: {:?}",
        again.decided
    );
}

/// A conversation that does not read is named in today's review and paged once for that set;
/// the review of the readable rest goes on.
#[test]
fn an_unreadable_conversation_is_named_and_paged_once() {
    use suzatary_core::secretary::review::note_unreadable;
    use suzatary_core::secretary::store::{ConversationId, Entry, Kind};
    use suzatary_core::secretary::transcript;
    let caps = Fakes::new("review-unreadable");
    let zone = suzatary_core::zone::Zone::UTC;
    assert_eq!(
        note_unreadable(&caps.secretary(), zone).map(|page| page.is_some()),
        Ok(false),
        "every conversation reads"
    );
    let bad = ConversationId::new("2026-01-02-1").expect("a slug");
    let at = caps.clock.now();
    let lines = caps.conversations.conversations();
    transcript::start(
        lines,
        &bad,
        &Entry::Opened {
            at,
            title: suzatary_core::secretary::words::Words::kept("Topic".to_owned()),
            model: "m".to_owned(),
            kind: Kind::Topic,
        },
    )
    .expect("opened");
    transcript::push(
        lines,
        &bad,
        &Entry::Rewound {
            to: 9,
            at,
            by: "ada".to_owned(),
        },
    )
    .expect("written");
    let page = note_unreadable(&caps.secretary(), zone).expect("lists");
    assert!(
        matches!(
            page,
            Some(suzatary_core::capability::Push::Unreadable { conversations, .. })
                if conversations.get() == 1
        ),
        "{page:?}"
    );
    assert_eq!(
        note_unreadable(&caps.secretary(), zone).map(|page| page.is_some()),
        Ok(false),
        "the same set pages once"
    );
    let today = prodrome_core::fpl::instant_of(caps.clock.now()).date();
    let round = suzatary_core::secretary::read(lines, &today_of(today)).expect("reads");
    let named = round
        .turns
        .iter()
        .filter(|said| matches!(&said.turn, suzatary_core::capability::Turn::Act { text, .. } if text.contains("2026-01-02-1")))
        .count();
    assert_eq!(named, 1, "named once in today's review");
}

/// A review that could not run leaves a record saying so, while the caller still exits zero.
#[test]
fn a_review_that_cannot_reach_its_model_writes_the_outage_down() {
    let caps = Fakes::new("review-unreachable");
    let _ = three_cards(&caps);
    let policy = common::chain();
    let deployment = deployment();
    // An empty script answers `ModelError::Failed`, as an unreachable model does.
    let caps = caps.model(FakeModel::saying([]));
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let refused = review(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    );
    assert!(refused.is_err(), "an unreachable model refuses the review");

    // The record names what nobody answered for.
    let today = prodrome_core::fpl::instant_of(caps.clock.now()).date();
    let stored =
        suzatary_core::secretary::read(caps.conversations.conversations(), &today_of(today))
            .expect("the conversation was opened before the model was asked");
    assert_eq!(
        stored.failures.len(),
        1,
        "one attempt, one line: {:?}",
        stored.failures
    );
    assert_eq!(
        stored.failures[0].standing, 3,
        "all three cards stood unanswered"
    );
    assert!(
        stored.failures[0]
            .reason
            .get()
            .is_some_and(|reason| !reason.is_empty()),
        "the box's own words for why, not an empty string"
    );
    assert!(
        stored.reviews.is_empty(),
        "nothing was decided, so nothing is a decision"
    );

    // It reads back through the sibling of `reviews`.
    let seen =
        suzatary_core::secretary::review::failures(caps.conversations.conversations(), today)
            .expect("the store reads")
            .items;
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1.standing, 3);
    assert!(
        reviews(caps.conversations.conversations(), today)
            .expect("the store reads")
            .items
            .is_empty(),
        "an outage is not a decision and must not show as one"
    );
}

/// A hold's reason is the answer's prose, not its Typst markup.
#[test]
fn a_hold_s_reason_is_the_answer_s_words_not_its_markup() {
    let caps = Fakes::new("review-held-prose");
    let _ = three_cards(&caps);
    let policy = common::chain();
    let holding = caps.model(FakeModel::saying([
        r#"{"say": "*Why I held it:* each one #minor[changes] something of yours."}"#,
    ]));
    let chains = [Bound {
        name: "life",
        caps: holding.secretary(),
        policy: &policy,
    }];
    let done = review(
        &holding.secretary(),
        &deployment(),
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the round runs");
    let held = done.with(Outcome::Held);
    assert!(!held.is_empty());
    for line in held {
        assert_eq!(
            line.why.get().map(String::as_str),
            Some("Why I held it: each one changes something of yours."),
            "{line:?}"
        );
    }
}

/// The review's round, pinned: its lines and what the model is shown and sent.
#[test]
fn the_reviews_round_is_pinned() {
    let caps = Fakes::new("review-pinned");
    let [_, duplicate, _] = three_cards(&caps);
    let policy = common::chain();
    let deployment = deployment();
    let caps = caps.model(script(&duplicate));
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let done = review(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the review runs");

    assert_eq!(done.rounds, 2);
    common::pinned("review", &common::scene(&caps, &done.stored.id));
}

/// Day `day`'s review conversation.
fn today_of(day: chrono::NaiveDate) -> suzatary_core::secretary::store::ConversationId {
    suzatary_core::secretary::store::ConversationId::daily(
        suzatary_core::secretary::daily::Daily::Review,
        day,
    )
}
