//! The filing door's laws, over the conversations and the inbox held in core's memory.

use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::refusal::Refusal;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::changes::Changeset;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::inbox::ProposalId;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::{Proposal, Reasons};
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_inbox::secretary::responded;
use suzatary_lines::secretary::transcript;
use suzatary_secretary::stores::InMemory;
use suzatary_topics::secretary::filed::*;

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 27, 9, minute, 0, 0).expect("an instant")
}

fn session<'a>(conversation: &'a ConversationId, proposer: Proposer, todo: &str) -> Filing<'a> {
    let serves: suzatary_base::reference::TodoRef = format!("todo:suzatary/{todo}")
        .parse()
        .expect("a todo reference");
    Filing {
        conversation,
        proposer,
        proposal: Proposal::Act(Act::CodingSession {
            task: suzatary_inbox::coding::Task::try_from(format!("Build {serves}."))
                .expect("a task"),
        }),
        reasons: Reasons::new(
            "asked",
            vec![suzatary_base::reference::Reference::Todo(serves.clone())],
        )
        .expect("reasons"),
        serves: Some(serves),
        revises: None,
    }
}

/// `todo` asked for as a new todo (0), an update that names a price (1) or a reprice (2), at
/// `percent`, with a rationale that differs on every pass.
fn priced(
    shape: u8,
    todo: &str,
    percent: u32,
    pass: usize,
) -> suzatary_inbox::secretary::changes::Act {
    use suzatary_inbox::secretary::changes::Act as ChainAct;
    use suzatary_record::todo::{Patch, Price, TodoFields, TodoPatch};
    let price = Price::Flat { percent };
    let rationale = format!("read on pass {pass}");
    match shape {
        0 => ChainAct::Add {
            todo: todo.to_owned(),
            fields: TodoFields {
                body: "Answer the chat request".to_owned(),
                rationale,
                kind: "todo".to_owned(),
                category: String::new(),
                price,
                detail: String::new(),
                waiting_on: String::new(),
            },
            from: suzatary_inbox::secretary::changes::Provenance::default(),
        },
        1 => ChainAct::Update {
            todo: todo.to_owned(),
            fields: TodoPatch {
                detail_append: Patch::Set("The sender asked again.".to_owned()),
                rationale: Patch::Set(rationale),
                price: Patch::Set(price),
                ..TodoPatch::default()
            },
        },
        _ => ChainAct::Reprice {
            todo: todo.to_owned(),
            priority: price,
            rationale,
        },
    }
}

fn resting_on(sources: &[&str]) -> Reasons {
    Reasons::new(
        "a message asks for it",
        sources
            .iter()
            .map(|day| format!("briefing:{day}").parse().expect("a reference"))
            .collect(),
    )
    .expect("reasons")
}

fn filed_triage(
    cards: &TriageCards<'_>,
    at: Datetime,
    act: suzatary_inbox::secretary::changes::Act,
    reasons: Reasons,
) -> Result<Carded, Refusal> {
    cards.file(
        at,
        Changeset::new("life", vec![act]).expect("one act"),
        reasons,
        None,
        &|_| Ok(()),
    )
}

proptest::proptest! {
    /// THE REFILE LOOP: the triage agent proposing the same request on every pass, its price
    /// and rationale read afresh each time, leaves exactly one standing card, the first, with
    /// the first pass's one act; no pass files, joins or withdraws.
    #[test]
    fn the_same_request_on_every_pass_is_one_standing_card(
        shape in 0u8..3,
        percents in proptest::collection::vec(1u32..100, 1..12),
    ) {
        let conversations = suzatary_secretary::stores::conversations::InMemory::default();
        let inbox = InMemory::<InboxEvent>::default();
        let cards = TriageCards::new(conversations.conversations(), &inbox);
        let mut first = None;
        for (pass, percent) in percents.iter().enumerate() {
            let carded = filed_triage(
                &cards,
                at(u32::try_from(pass).expect("a minute")),
                priced(shape, "chatrequest", *percent, pass),
                resting_on(&["2026-11-03"]),
            )
            .expect("filed");
            let first = first.get_or_insert_with(|| carded.card.clone());
            proptest::prop_assert_eq!(&carded.card, &*first);
            proptest::prop_assert_eq!(carded.acts, 1);
            proptest::prop_assert_eq!(
                carded.how,
                if pass == 0 { Carding::New } else { Carding::Already }
            );
        }
        let standing = suzatary_secretary::secretary::verbs::standing(&inbox).expect("the inbox reads");
        proptest::prop_assert_eq!(standing.len(), 1, "{:?}", standing);
        proptest::prop_assert_eq!(inbox::fold(&inbox).expect("reads").objects().count(), 1);
    }
}

/// A request somebody answered is not asked again on the same sources at another price, and
/// is asked again, as a new card, on a source the answered card did not rest on.
#[test]
fn an_answered_request_comes_back_only_on_a_new_source() {
    let conversations = suzatary_secretary::stores::conversations::InMemory::default();
    let inbox = InMemory::<InboxEvent>::default();
    let cards = TriageCards::new(conversations.conversations(), &inbox);
    let first = filed_triage(
        &cards,
        at(0),
        priced(2, "permit", 62, 0),
        resting_on(&["2026-11-02"]),
    )
    .expect("filed");
    let id = inbox::fold(&inbox)
        .expect("reads")
        .objects()
        .find(|object| object.asked.asked == first.card)
        .expect("the card")
        .id
        .clone();
    inbox::withdraw(
        &inbox,
        &id,
        &transcript::writer(),
        at(1),
        responded::Withdrawal::NoLongerNeeded {
            why: "a repricing with no new fact".to_owned(),
        },
    )
    .expect("withdrawn");
    let refused = filed_triage(
        &cards,
        at(2),
        priced(2, "permit", 61, 1),
        resting_on(&["2026-11-02"]),
    )
    .expect_err("the same request on the same source");
    assert!(
        refused.message().contains(&first.card.to_string()),
        "{}",
        refused.message()
    );
    let again = filed_triage(
        &cards,
        at(3),
        priced(2, "permit", 40, 2),
        resting_on(&["2026-11-02", "2026-11-03"]),
    )
    .expect("a new source is a new request");
    assert_eq!(again.how, Carding::New);
    assert_ne!(again.card, first.card);
}

/// The limit is the filing door's, under the inbox's hold: with one build in progress, a
/// build card and the secretary's session filed at the same instant cannot both pass.
#[test]
fn two_filings_at_once_take_one_free_slot() {
    let conversations = suzatary_secretary::stores::conversations::InMemory::default();
    let inbox = InMemory::<InboxEvent>::default();
    let (build, default) = (
        ConversationId::new("build").expect("a slug"),
        ConversationId::new("default").expect("a slug"),
    );
    for conversation in [&build, &default] {
        opened(
            conversations.conversations(),
            conversation,
            "Builds",
            &Proposer::Box,
            at(0),
        )
        .expect("opened");
    }
    file(
        conversations.conversations(),
        &inbox,
        &[],
        at(1),
        session(&build, Proposer::Box, "first"),
    )
    .expect("the first build files");
    let barrier = std::sync::Barrier::new(2);
    let filed: Vec<Result<(Filed, ProposalId), Refusal>> = std::thread::scope(|scope| {
        let threads = [
            (&build, Proposer::Box, "second"),
            (&default, Proposer::Secretary, "third"),
        ]
        .map(|(conversation, proposer, todo)| {
            let (conversations, inbox, barrier) = (&conversations, &inbox, &barrier);
            scope.spawn(move || {
                barrier.wait();
                file(
                    conversations.conversations(),
                    inbox,
                    &[],
                    at(2),
                    session(conversation, proposer, todo),
                )
            })
        });
        threads
            .into_iter()
            .map(|thread| thread.join().expect("the thread ran"))
            .collect()
    });
    assert_eq!(
        filed.iter().filter(|filed| filed.is_ok()).count(),
        1,
        "{filed:?}"
    );
    let refused = filed
        .iter()
        .find_map(|filed| filed.as_ref().err())
        .expect("one refused");
    assert!(
        refused.message().contains("2 todos are in progress"),
        "{}",
        refused.message()
    );
}
