//! `add_topics`, called as the secretary calls it, over real topic stores: a round of the owner's
//! words alone may make a topic, and a round that has read mail may only use one in use, so mail
//! never names a topic.

mod common;

use common::Fakes;
use serde_json::{Value, json};
use suzatary_core::capability::{ToolCall, Turn};
use suzatary_core::secretary::store::{ConversationId, DEFAULT, Entry};
use suzatary_core::secretary::topics::target::Target;
use suzatary_core::secretary::topics::topic::Topic;

fn call(fakes: &Fakes, args: Value) -> Result<Value, suzatary_core::refusal::Refusal> {
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
        &DEFAULT.clone(),
        "add_topics",
        &args,
    )
    .map(|answer| answer.value)
}

/// Writes `turns` to the default conversation, each a person's turn stamped as the owner's.
fn say(fakes: &Fakes, turns: Vec<Turn>) {
    let conversations = fakes.secretary().conversations;
    let at = |minute: u32| {
        prodrome_core::literal::Datetime::new(2026, 10, 24, 9, minute, 0, 0).expect("an instant")
    };
    let id: &ConversationId = &DEFAULT;
    if suzatary_core::secretary::read(conversations, id).is_err() {
        suzatary_core::secretary::transcript::start(
            conversations,
            id,
            &Entry::Opened {
                at: at(0),
                title: suzatary_core::secretary::words::Words::kept("Secretary".to_owned()),
                model: "scripted".to_owned(),
                kind: suzatary_core::secretary::store::Kind::Default,
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
    assert_eq!(made["on"], "round:default/0");
    assert_eq!(made["new"], json!(["garden"]));
    assert_eq!(made["existing"], "none yet");
    assert_eq!(topics_on(&fakes, "round:default/0"), ["garden"]);

    say(&fakes, vec![answered("Tulips."), asked("and the hedge?")]);
    for twins in [json!(["gardens"]), json!(["trip-plan", "plans-trip"])] {
        let twin = call(&fakes, json!({ "topics": twins })).expect_err("a near twin is refused");
        assert!(twin.message().contains("near twin"), "{}", twin.message());
    }
    let three = call(&fakes, json!({"topics": ["garden", "hedge", "trip"]})).expect("three fit");
    assert_eq!(three["on"], "round:default/2");
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
        again["on"], answering,
        "the round being answered, not the later message"
    );
    assert_eq!(again["new"], json!([]));
    assert_eq!(again["existing"], "garden (2), hedge (1), trip (1)");
    assert_eq!(topics_on(&fakes, answering), ["garden"]);
    assert!(topics_on(&fakes, "round:default/8").is_empty());
}
