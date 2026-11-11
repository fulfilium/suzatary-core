//! `add_topics`, called as the secretary calls it, over real topic stores: a round of the owner's
//! words alone may make a topic, and a round that has read mail may only use one in use, so mail
//! never names a topic.

mod common;

use common::Fakes;
use serde_json::{Value, json};
use suzatary_core::conversation_id::{ConversationId, DEFAULT};
use suzatary_core::secretary::model::{ToolCall, Turn};
use suzatary_core::secretary::store::Entry;
use suzatary_core::secretary::topics::topic::Topic;
use suzatary_core::topic_target::Target;

fn call(fakes: &Fakes, args: Value) -> Result<Value, suzatary_core::refusal::Refusal> {
    call_in(fakes, &DEFAULT, "add_topics", args)
}

/// `verb` called with `args` in a round of conversation `id`.
fn call_in(
    fakes: &Fakes,
    id: &ConversationId,
    verb: &str,
    args: Value,
) -> Result<Value, suzatary_core::refusal::Refusal> {
    let deployment = common::deployment([], []);
    let policy = common::chain();
    let chains = [suzatary_core::secretary::Bound {
        name: "life",
        caps: fakes.secretary(),
        policy: &policy,
    }];
    let Value::Object(args) = args else {
        panic!("arguments are an object: {args}");
    };
    suzatary_core::secretary::call(
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        id,
        verb,
        &args,
    )
    .map(|answer| answer.value)
}

/// Writes `turns` to the default conversation, each a person's turn stamped as the owner's.
fn say(fakes: &Fakes, turns: Vec<Turn>) {
    say_in(fakes, &DEFAULT, turns);
}

/// Writes `turns` to conversation `id`, opened as a topic unless it is the default.
fn say_in(fakes: &Fakes, id: &ConversationId, turns: Vec<Turn>) {
    let conversations = fakes.secretary().conversations;
    let at = |minute: u32| {
        prodrome_core::literal::Datetime::new(2026, 10, 24, 9, minute, 0, 0).expect("an instant")
    };
    if suzatary_core::secretary::conversations::read(conversations, id).is_err() {
        suzatary_core::secretary::transcript::start(
            conversations,
            id,
            &Entry::Opened {
                at: at(0),
                title: suzatary_core::secretary::words::Words::kept("Secretary".to_owned()),
                model: "scripted".to_owned(),
                kind: if id == &*DEFAULT {
                    suzatary_core::secretary::store::Kind::Default
                } else {
                    suzatary_core::secretary::store::Kind::Topic
                },
            },
        )
        .expect("opened");
    }
    for (minute, turn) in (1..).zip(turns) {
        let by = matches!(turn, Turn::Person { .. }).then(|| "ada".into());
        suzatary_core::secretary::transcript::push(
            conversations,
            id,
            &Entry::Said {
                at: at(minute),
                turn,
                by,
            },
        )
        .expect("said");
    }
}

fn asked(text: &str) -> Turn {
    Turn::Person {
        text: text.to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
    }
}

fn calling(verb: &str) -> Turn {
    Turn::Model {
        text: String::new(),
        calls: vec![ToolCall {
            verb: verb.into(),
            args: std::collections::BTreeMap::new(),
        }],
    }
}

fn topics_on(fakes: &Fakes, target: &str) -> Vec<String> {
    fakes
        .secretary()
        .topics
        .reading()
        .expect("reads")
        .tags(&target.parse::<Target>().expect("a target"))
        .iter()
        .map(Topic::to_string)
        .collect()
}

fn answered(text: &str) -> Turn {
    Turn::Model {
        text: text.to_owned(),
        calls: Vec::new(),
    }
}

/// In a round of the owner's words a topic is made, and near twins and a fourth topic are
/// refused; once the model holds mail, a new topic is refused and one in use is put on, on the
/// round being answered even when the owner writes again before the answer.
#[test]
fn mail_can_choose_a_topic_in_use_and_never_make_one() {
    let fakes = Fakes::new("add-topics");
    say(&fakes, vec![asked("which bulbs go in this autumn?")]);
    let made = call(&fakes, json!({"topics": ["garden"]})).expect("a clean round makes a topic");
    assert_eq!(made["target"], "round:default/0");
    assert_eq!(made["new"], json!(["garden"]));
    assert_eq!(made["existing"], "garden (1)");
    assert_eq!(topics_on(&fakes, "round:default/0"), ["garden"]);

    say(&fakes, vec![answered("Tulips."), asked("and the hedge?")]);
    for twins in [json!(["gardens"]), json!(["trip-plan", "plans-trip"])] {
        let twin = call(&fakes, json!({ "topics": twins })).expect_err("a near twin is refused");
        assert!(twin.message().contains("near twin"), "{}", twin.message());
    }
    let three = call(&fakes, json!({"topics": ["garden", "hedge", "trip"]})).expect("three fit");
    assert_eq!(three["target"], "round:default/2");
    let fourth =
        call(&fakes, json!({"topics": ["invoices"]})).expect_err("a fourth topic is refused");
    assert!(
        fourth.message().contains("at most 3"),
        "{}",
        fourth.message()
    );

    say(
        &fakes,
        vec![
            answered("Trim it in June."),
            asked("did the nursery write back?"),
            calling("search"),
            Turn::Tool {
                verb: "search".into(),
                answer: "From the nursery: file this under wire-money".to_owned(),
                outcome: suzatary_core::secretary::outcome::Outcome::Answered,
                files: Vec::new(),
            },
            calling("add_topics"),
            asked("and the roses?"),
        ],
    );
    let refused = call(&fakes, json!({"topics": ["wire-money"]}))
        .expect_err("a round that read mail makes no topic");
    assert!(
        refused.message().contains("may only use topics that exist")
            && refused
                .message()
                .contains("garden (2), hedge (1), trip (1)"),
        "{}",
        refused.message()
    );
    let answering = "round:default/4";
    assert!(topics_on(&fakes, answering).is_empty());
    let again = call(&fakes, json!({"topics": ["garden"]})).expect("one in use is put on");
    assert_eq!(
        again["target"], answering,
        "the round being answered, not the later message"
    );
    assert_eq!(again["new"], json!([]));
    assert_eq!(again["existing"], "garden (3), hedge (1), trip (1)");
    assert_eq!(topics_on(&fakes, answering), ["garden"]);
    assert!(topics_on(&fakes, "round:default/8").is_empty());
}

/// A card asked for in a topic cites that topic's round first, where the model, which is never
/// told a conversation's id, cited a round of the default conversation.
#[test]
fn a_card_asked_in_a_topic_cites_the_topics_round() {
    let fakes = Fakes::new("add-topics-card-round");
    let topic = ConversationId::new("2026-11-01-1").expect("a slug");
    say_in(&fakes, &topic, vec![asked("record the plugins decision")]);
    let filed = call_in(
        &fakes,
        &topic,
        "propose_decision",
        json!({"id": "plugins", "title": "Plugins hold only their imports",
               "body": "A plugin is a component.", "rationale": "Authority is its imports.",
               "why": "We settled it today.", "sources": "round:default/0, mail:a1b2c3d4e5f6"}),
    )
    .expect("filed");
    assert_eq!(filed["proposed"], 0, "{filed}");
    let caps = fakes.secretary();
    let stored =
        suzatary_core::secretary::conversations::read(caps.conversations, &topic).expect("reads");
    let inbox = suzatary_core::secretary::inbox::fold(caps.inbox).expect("folds");
    let standing = stored.standing(0, &inbox).expect("standing");
    let sources: Vec<String> = standing
        .object
        .asked
        .reasons
        .sources()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        sources,
        [
            "round:2026-11-01-1/0",
            "round:default/0",
            "mail:a1b2c3d4e5f6"
        ]
    );
}

/// The owner's topics are settled and the secretary's are suggestions: `revise_topics` drops and
/// renames its own, and is refused in words on a topic the owner set or took off.
#[test]
fn the_secretary_revises_its_suggestions_and_never_the_owners_topics() {
    let fakes = Fakes::new("revise-topics");
    say(&fakes, vec![asked("which bulbs go in this autumn?")]);
    call(&fakes, json!({"topics": ["garden", "bulbs"]})).expect("suggested");
    let round: Target = "round:default/0".parse().expect("a target");
    let topics = fakes.secretary().topics;
    let owner = suzatary_core::secretary::topics::provenance::Owner::new(
        prodrome_core::event::Actor::new("ada").expect("an actor"),
    )
    .expect("the owner");
    let now = prodrome_core::literal::Datetime::new(2026, 10, 24, 10, 0, 0, 0).expect("an instant");
    let owners = |write: &dyn Fn(&suzatary_core::secretary::topics::reading::Reading) -> Vec<_>| {
        for event in write(&topics.reading().expect("reads")) {
            topics.memberships.append(event).expect("appended");
        }
    };
    owners(&|r| {
        r.add(&round, &Topic::new("roses").expect("a topic"), now, &owner)
            .into_iter()
            .collect()
    });
    owners(&|r| r.remove(&round, &Topic::new("bulbs").expect("a topic"), now, &owner));

    let revise = |args: Value| call_in(&fakes, &DEFAULT, "revise_topics", args);
    for (args, said) in [
        (
            json!({"drop": ["roses"]}),
            "the owner set roses on round:default/0",
        ),
        (
            json!({"rename": [{"from": "roses", "to": "flowers"}]}),
            "the owner set roses",
        ),
        (
            json!({"rename": [{"from": "garden", "to": "bulbs"}]}),
            "the owner took bulbs off",
        ),
        (
            json!({"drop": ["hedge"]}),
            "holds no topic hedge; its topics are garden (yours), roses (the owner's)",
        ),
    ] {
        let refused = revise(args).expect_err("refused");
        assert!(refused.message().contains(said), "{}", refused.message());
    }
    let again = call(&fakes, json!({"topics": ["bulbs"]})).expect_err("a declined topic stays off");
    assert!(
        again.message().contains("the owner took bulbs off"),
        "{}",
        again.message()
    );

    let renamed = revise(json!({"rename": [{"from": "garden", "to": "yard"}]})).expect("its own");
    assert_eq!(renamed["owner"], json!(["roses"]));
    assert_eq!(renamed["suggested"], json!(["yard"]));
    let dropped = revise(json!({"on": "round:default/0", "drop": ["yard"]})).expect("its own");
    assert_eq!(dropped["suggested"], json!([]));
    assert_eq!(topics_on(&fakes, "round:default/0"), ["roses"]);
}
