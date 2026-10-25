//! THE TRUST BOUNDARY: the triage agent reads mail anyone can send, so its writing verbs file
//! cards in the Inbox and append nothing to any chain; only the owner's tap performs a card.

use prodrome_core::event::Actor;
use prodrome_core::policy::Untrusted;
use serde_json::{Map, Value, json};
use suzatary_core::capability::{Agent, Nothing};
use suzatary_core::chains::{ChainName, ChainPolicy, Chains};
use suzatary_core::deployment::Deployment;
use suzatary_core::record::TodoEvent;
use suzatary_core::secretary::changes::Act;
use suzatary_core::secretary::filed::TriageCards;
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::proposer::Proposer;
use suzatary_core::secretary::store::Proposal;
use suzatary_core::store::Reads as _;
use suzatary_core::store::{InMemory, Prodrome};

fn deployment() -> Deployment {
    let actor = |name: &str| Actor::new(name).expect("a roster name");
    let chain = ChainPolicy {
        name: ChainName::new("life").expect("a slug"),
        untrusted: Untrusted::of([actor("triage")]),
        roster: [actor("owner"), actor("triage")].into_iter().collect(),
        root: suzatary_core::cast::keys::root(),
    };
    Deployment {
        viewers: std::collections::BTreeSet::new(),
        actors: std::collections::BTreeMap::new(),
        chains: Chains::of(vec![chain]).expect("one chain"),
        categories: [("home".to_owned(), 1)].into_iter().collect(),
        app_url: "https://example.test".to_owned(),
        coding: None,
        zone: suzatary_core::zone::Zone::UTC,
    }
}

fn args(value: &Value) -> Map<String, Value> {
    value.as_object().expect("an object").clone()
}

/// A chain holding one todo of the owner's, `alpha`.
fn the_owners_chain() -> InMemory<TodoEvent> {
    let chain = InMemory::<TodoEvent>::default();
    chain
        .append(
            suzatary_core::authoring::build_todo(
                &suzatary_core::authoring::NewTodo {
                    todo: "alpha".to_owned(),
                    fields: suzatary_core::todo::TodoFields {
                        body: "Renew the permit".to_owned(),
                        rationale: "an errand".to_owned(),
                        kind: "todo".to_owned(),
                        category: "home".to_owned(),
                        price: suzatary_core::todo::Price::Flat { percent: 75 },
                        detail: String::new(),
                        waiting_on: String::new(),
                    },
                    source: None,
                    subtodos: Vec::new(),
                },
                suzatary_core::zone::Moment {
                    at: prodrome_core::literal::Datetime::new(2026, 9, 1, 9, 0, 0, 0)
                        .expect("an instant"),
                    zone: suzatary_core::zone::Zone::UTC,
                },
                "owner",
                &["home"],
                &Nothing,
            )
            .expect("a todo"),
        )
        .expect("the owner's todo");
    chain
}

/// Every write the agent can spell files a card or is refused, the chain holds exactly what it
/// held before, and a later write about the same todo joins the open card.
#[test]
fn the_agents_writes_are_cards_and_reach_no_chain() {
    let chain = the_owners_chain();
    let before = chain.objects().expect("reads");
    let records = [(
        "life".to_owned(),
        Agent {
            chain: &chain,
            ..Nothing::agent()
        },
    )]
    .into_iter()
    .collect();
    let conversations = suzatary_core::secretary::conversations::InMemory::default();
    let inbox = InMemory::<InboxEvent>::default();
    let cards = TriageCards::new(conversations.conversations(), &inbox);
    let deployment = deployment();
    let call = |verb: &str, value: Value| {
        suzatary_core::agent::call(&records, &cards, &deployment, verb, &args(&value))
    };
    let reasons = json!({"why": "a message asks for it", "sources": "briefing:2026-09-02"});
    let with = |mut fields: Value| {
        for (key, value) in reasons.as_object().expect("an object") {
            fields[key] = value.clone();
        }
        fields
    };

    let added = call(
        "todo_add",
        with(
            json!({"todo_id": "beta", "body": "File the form", "rationale": "a form",
                    "category": "home", "priority": "70%"}),
        ),
    )
    .expect("filed");
    assert_eq!(added["filed"], "proposal:triage/0");
    let joined = call(
        "record",
        with(json!({"kind": "completed", "todo": "beta", "note": "filed"})),
    )
    .expect("joined");
    assert_eq!(joined["acts"], 2, "{joined}");
    call(
        "todo_update",
        with(json!({"todo_id": "alpha", "body": "Renew the parking permit"})),
    )
    .expect("filed");
    assert!(
        call(
            "todo_add",
            json!({"todo_id": "gamma", "body": "x", "rationale": "y", "category": "home",
                   "priority": "70%"})
        )
        .is_err(),
        "a card without its reasons is refused"
    );
    assert!(
        call(
            "record",
            with(json!({"kind": "completed", "todo": "nosuchtodo"}))
        )
        .is_err(),
        "a card the tap would refuse is refused now"
    );

    assert_eq!(
        chain.objects().expect("reads"),
        before,
        "the chain holds exactly what it held"
    );
    let fold = suzatary_core::secretary::inbox::fold(&inbox).expect("reads");
    assert!(
        fold.objects()
            .all(|object| object.asked.proposer == Proposer::Triage),
        "every card says the triage agent asked"
    );
    let mut open: Vec<Vec<&str>> = fold
        .objects()
        .filter(|object| object.reading.performs())
        .map(|object| match &object.asked.proposal {
            Proposal::Changes(changeset) => changeset.acts().iter().map(Act::word).collect(),
            other => panic!("a triage card is a changeset, not {other:?}"),
        })
        .collect();
    open.sort();
    assert_eq!(
        open,
        vec![vec!["add", "complete"], vec!["update"]],
        "one open card per todo; the joined one replaced the first"
    );
}
