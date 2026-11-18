//! The conversation rules' laws, over the conversations held in core's memory.

use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, Kind, Namer, Title};
use suzatary_lines::secretary::transcript;
use suzatary_lines::secretary::transcript::Transcript;
use suzatary_topics::secretary::rules::*;
use suzatary_triage::rule::{Engine, Named};

fn id() -> ConversationId {
    ConversationId::new("2026-10-05-1").expect("a slug")
}

fn store() -> suzatary_core::stores::conversations::InMemory {
    let store = suzatary_core::stores::conversations::InMemory::default();
    transcript::start(
        store.conversations(),
        &id(),
        &Entry::Opened {
            at: suzatary_base::capability::Datetime::new(2026, 10, 5, 9, 0, 0, 0)
                .expect("an instant"),
            title: suzatary_memory::secretary::words::Words::kept(String::new()),
            model: "m".to_owned(),
            kind: Kind::Topic,
        },
    )
    .expect("opens");
    store
}

fn say(store: &suzatary_core::stores::conversations::InMemory, turn: Turn) {
    transcript::push(
        store.conversations(),
        &id(),
        &Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 10, 5, 9, 1, 0, 0)
                .expect("an instant"),
            turn,
            by: None,
        },
    )
    .expect("said");
}

fn read(store: &suzatary_core::stores::conversations::InMemory) -> std::sync::Arc<Transcript> {
    transcript::read(store.conversations(), &id()).expect("reads")
}

fn person(text: &str) -> Turn {
    Turn::Person {
        text: text.to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
    }
}

fn answer(text: &str) -> Turn {
    Turn::Model {
        text: text.to_owned(),
        calls: Vec::new(),
    }
}

/// Each rule fires once on the reading that owes it, never again on it, and its outcome
/// discharges it, so a restarted engine over the store fires nothing: the memo that lasts is
/// the transcript.
#[test]
fn each_rule_is_discharged_by_its_own_outcome() {
    let lines = store();
    let mut engine = Engine::new(RULES.to_vec());
    assert!(engine.due(&id(), &read(&lines)).is_none(), "nothing said");

    say(&lines, person("when is the permit due"));
    let (rule, firing) = engine.due(&id(), &read(&lines)).expect("an answer owed");
    assert_eq!(rule, Conversational::Answer);
    assert_eq!(firing.on(), &read(&lines).name());
    assert!(
        engine.due(&id(), &read(&lines)).is_none(),
        "a second notice of one reading is the first"
    );

    say(&lines, answer("on Friday"));
    let (rule, _) = engine
        .due(&id(), &read(&lines))
        .expect("the answer leaves the topic unnamed");
    assert_eq!(rule, Conversational::Title);
    transcript::push(
        lines.conversations(),
        &id(),
        &Entry::Titled {
            at: suzatary_base::capability::Datetime::new(2026, 10, 5, 9, 2, 0, 0)
                .expect("an instant"),
            title: suzatary_memory::secretary::words::Words::kept(
                Title::new("The permit").expect("a title"),
            ),
            by: Namer::Job {
                model: "m".to_owned(),
            },
        },
    )
    .expect("titled");
    assert!(engine.due(&id(), &read(&lines)).is_none());
    assert!(
        Engine::new(RULES.to_vec())
            .due(&id(), &read(&lines))
            .is_none(),
        "a restart refires nothing"
    );
}

/// A line written while a round runs is a new reading, owed again.
#[test]
fn a_new_line_is_a_new_reading() {
    let lines = store();
    let mut engine = Engine::new(vec![Conversational::Answer]);
    say(&lines, person("first"));
    let first = engine.due(&id(), &read(&lines)).expect("owed").1;
    say(&lines, person("second"));
    let second = engine.due(&id(), &read(&lines)).expect("owed again").1;
    assert_ne!(first.on(), second.on());
}
