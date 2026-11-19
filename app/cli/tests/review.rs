//! The secretary reviewing the triage agent's cards end to end, with only the model scripted.
//!
//! No server or peer: the box starts a review, so no request, gate or `Tap` is involved.

use crate::common;

use common::{FakeModel, Fakes};
use suzatary_base::capability::Clock as _;
use suzatary_base::reference::ProposalRef;
use suzatary_cards::notify::Push;
use suzatary_conversations::secretary::model;
use suzatary_conversations::secretary::store::{Outcome, Settle};
use suzatary_inbox::secretary::changes::{Act, Changeset, Provenance};
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::proposal::Reasons;
use suzatary_lines::secretary::conversations;
use suzatary_secretary::secretary::Bound;
use suzatary_secretary::secretary::daily::Run;
use suzatary_secretary::secretary::review::{Noticed, Pass, Reviewed, pass, reviews};
use suzatary_store::store::Reads as _;
use suzatary_topics::secretary::filed::TriageCards;

/// The review of the cards standing run to its end; a scripted model is under no limit, so it
/// always starts.
fn review(
    caps: &suzatary_secretary::secretary::Secretary<'_>,
    deployment: &suzatary_connect::deployment::Deployment,
    settables: &suzatary_connect::settings::Settables,
    chains: &[Bound<'_>],
    system: &str,
) -> Result<Reviewed, suzatary_base::refusal::Refusal> {
    match pass(caps, deployment, settables, chains, system)? {
        Pass::Cards(noticed) => noticed.run.map(|run| match run {
            Run::Done(reviewed) => reviewed,
            Run::Paused(limited) => panic!("a scripted model is never limited: {limited}"),
        }),
        Pass::Nothing => panic!("every review here has a card standing"),
    }
}

/// Three cards, as the triage agent files them: two adds and a completion. Their lines go to
/// a store the review does not read, since the review reads cards from the inbox alone.
fn three_cards(caps: &Fakes) -> [ProposalRef; 3] {
    let lines = suzatary_stores::stores::conversations::InMemory::default();
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
        fields: suzatary_record::todo::TodoFields {
            body: "Renew the parking permit".to_owned(),
            rationale: "an errand with a recoverable cost".to_owned(),
            kind: "todo".to_owned(),
            category: "home".to_owned(),
            price: suzatary_record::todo::Price::Flat { percent: 75 },
            detail: String::new(),
            waiting_on: String::new(),
        },
        from: Provenance::default(),
    }
}

/// The fixture deployment, which `context` and the chain lookup read.
fn deployment() -> suzatary_connect::deployment::Deployment {
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
    inbox::fold(&caps.inbox)
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
        &suzatary_connect::settings::Settables::default(),
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

/// Law: the brief offers back each card the secretary withdrew that day, once, as a restore card
/// whose tap applies the withdrawn card's changes; a withdrawal older than a day is not offered,
/// and the withdrawn card itself never stands again.
#[test]
fn the_brief_offers_the_day_s_withdrawals_back_once() {
    use suzatary_inbox::secretary::acts::Act as BoxAct;
    use suzatary_inbox::secretary::proposal::Proposal;
    use suzatary_inbox::secretary::proposer::Proposer;
    let caps = Fakes::new("review-undo");
    let [_, duplicate, _] = three_cards(&caps);
    let policy = common::chain();
    let caps = caps.model(script(&duplicate));
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    review(
        &caps.secretary(),
        &deployment(),
        &suzatary_connect::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the review runs");
    let secretary = caps.secretary();
    let brief = suzatary_base::conversation_id::DEFAULT.clone();
    conversations::read_or_open_default(
        secretary.conversations,
        secretary.model,
        &brief,
        caps.clock.now(),
    )
    .expect("the brief's conversation opens");
    let offer = |at| {
        suzatary_cards::secretary::undo::offer_withdrawn(
            secretary.conversations,
            &caps.inbox,
            &chains
                .iter()
                .map(suzatary_secretary::secretary::Bound::chain)
                .collect::<Vec<_>>(),
            &brief,
            at,
        )
        .expect("offers")
    };
    let later = prodrome_core::fpl::datetime_of(
        prodrome_core::fpl::instant_of(caps.clock.now()) + chrono::Duration::days(2),
    )
    .expect("an instant");
    assert_eq!(offer(later), 0, "a withdrawal two days old is not offered");
    assert_eq!(offer(caps.clock.now()), 1);
    assert_eq!(offer(caps.clock.now()), 0, "offered once");
    let fold = inbox::fold(&caps.inbox).expect("the inbox reads");
    let back = fold
        .objects()
        .find(|object| object.asked.proposer == Proposer::Secretary)
        .expect("the card offered back");
    assert_eq!(
        back.asked.proposal,
        Proposal::Act(BoxAct::Restore {
            card: duplicate.clone()
        })
    );
    assert!(back.reading.performs(), "the owner can tap it");
    assert!(
        !open(&caps, &duplicate),
        "the withdrawn card stays withdrawn"
    );
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
        &suzatary_connect::settings::Settables::default(),
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
            model::Turn::Tool {
                verb,
                outcome: suzatary_conversations::secretary::outcome::Outcome::Refused,
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

/// THE BOUND ON THE NOTICE: with no card standing, the review pass reads no object of the lines
/// (a write would read their heads) and asks no model, so it names no conversation that stopped
/// reading; the day's sweep names it in its one run, and pages for that set once.
#[test]
fn with_no_card_standing_the_review_reads_no_line_and_the_sweep_names_the_unreadable() {
    let caps = Fakes::new("review-quiet");
    let bad = unreadable(&caps);
    let policy = common::chain();
    let (deployment, settables) = (
        deployment(),
        suzatary_connect::settings::Settables::default(),
    );
    let secretary = caps.secretary();
    let counted = suzatary_store::store::Counted::new(secretary.conversations.lines);
    let mut counting = secretary;
    counting.conversations.lines = &counted;
    let chains = [Bound {
        name: "life",
        caps: counting,
        policy: &policy,
    }];

    let done = pass(&counting, &deployment, &settables, &chains, "a brief");
    assert_eq!(done, Ok(Pass::Nothing));
    assert_eq!(counted.reads(), 0, "no object of the lines was read");
    assert!(caps.model.asked().is_empty(), "the model was asked nothing");

    let sweep = || {
        suzatary_secretary::secretary::sweep::pass(
            &secretary,
            &deployment,
            &settables,
            &chains,
            "a",
        )
    };
    let swept = sweep();
    assert!(
        matches!(&swept.unreadable, Ok(Some(Push::Unreadable { conversations, .. }))
            if conversations.get() == 1),
        "{:?}",
        swept.unreadable
    );
    assert!(matches!(swept.run, Ok(Run::Done(_))), "{:?}", swept.run);
    assert_eq!(sweep().unreadable, Ok(None), "the same set pages once");
    assert_eq!(named(&caps, &bad), 1, "named once in today's review");
}

/// A conversation that does not read: a rewind to a turn never said.
fn unreadable(caps: &Fakes) -> suzatary_base::conversation_id::ConversationId {
    use suzatary_conversations::secretary::store::{Entry, Kind};
    use suzatary_lines::secretary::transcript;
    let bad = suzatary_base::conversation_id::ConversationId::new("2026-01-02-1").expect("a slug");
    let at = caps.clock.now();
    let lines = caps.conversations.conversations();
    transcript::start(
        lines,
        &bad,
        &Entry::Opened {
            at,
            title: suzatary_memory::secretary::words::Words::kept("Topic".to_owned()),
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
    bad
}

/// How many notes in today's review name `id`.
fn named(caps: &Fakes, id: &suzatary_base::conversation_id::ConversationId) -> usize {
    let today = prodrome_core::fpl::instant_of(caps.clock.now()).date();
    let round = conversations::read(caps.conversations.conversations(), &today_of(today))
        .expect("today's review reads");
    round
        .turns
        .iter()
        .filter(|said| {
            matches!(&said.turn, model::Turn::Act { text, .. } if text.contains(id.as_str()))
        })
        .count()
}

/// A second review of cards still held asks no model and writes no second line, so a pass with
/// nothing new costs no round.
#[test]
fn holding_the_same_card_again_is_not_a_second_line() {
    let caps = Fakes::new("review-twice");
    let [_, _, completion] = three_cards(&caps);
    let policy = common::chain();
    let deployment = deployment();
    let caps = caps.model(FakeModel::saying([
        r#"{"say": "These all change something of yours; I am leaving them."}"#,
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
            &suzatary_connect::settings::Settables::default(),
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
    assert_eq!(again.rounds, 0, "nothing new was standing");
    assert_eq!(again.standing, 3);
    assert!(
        again.decided.is_empty(),
        "a card already held today is already recorded: {:?}",
        again.decided
    );
}

/// While the model is under a limit, the review and the sweep do not start: they ask the model
/// nothing and write no line, so a limit fills no conversation with failed rounds.
#[test]
fn a_limited_model_starts_no_box_round_and_writes_nothing() {
    let caps = Fakes::new("review-limited");
    let _ = three_cards(&caps);
    let policy = common::chain();
    let deployment = deployment();
    let limited = suzatary_health::health::Limited {
        limit: suzatary_health::health::Limit::Usage,
        until: prodrome_core::fpl::instant_of(caps.clock.now()) + chrono::Duration::hours(3),
    };
    let caps = caps.model(FakeModel::saying([]).limited(limited));
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let lines = || {
        caps.conversations
            .conversations()
            .lines
            .objects()
            .expect("the lines read")
            .len()
    };
    let before = lines();
    let reviewed = pass(
        &caps.secretary(),
        &deployment,
        &suzatary_connect::settings::Settables::default(),
        &chains,
        "a brief",
    );
    assert_eq!(
        reviewed,
        Ok(Pass::Cards(Box::new(Noticed {
            unreadable: Ok(None),
            run: Ok(Run::Paused(limited)),
        })))
    );
    let swept = suzatary_secretary::secretary::sweep::pass(
        &caps.secretary(),
        &deployment,
        &suzatary_connect::settings::Settables::default(),
        &chains,
        "a brief",
    );
    assert!(matches!(swept.run, Ok(Run::Paused(paused)) if paused == limited));
    assert_eq!(lines(), before, "no line was written");
    assert!(caps.model.asked().is_empty(), "the model was asked nothing");
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
        &suzatary_connect::settings::Settables::default(),
        &chains,
        "a brief",
    );
    assert!(refused.is_err(), "an unreachable model refuses the review");

    let today = prodrome_core::fpl::instant_of(caps.clock.now()).date();
    let stored = conversations::read(caps.conversations.conversations(), &today_of(today))
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

    let seen =
        suzatary_secretary::secretary::review::failures(caps.conversations.conversations(), today)
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
        &suzatary_connect::settings::Settables::default(),
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
        &suzatary_connect::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the review runs");

    assert_eq!(done.rounds, 2);
    common::pinned("review", &common::scene(&caps, &done.stored.id));
}

/// Day `day`'s review conversation.
fn today_of(day: chrono::NaiveDate) -> suzatary_base::conversation_id::ConversationId {
    suzatary_base::conversation_id::ConversationId::daily(
        suzatary_base::conversation_id::Daily::Review,
        day,
    )
}
