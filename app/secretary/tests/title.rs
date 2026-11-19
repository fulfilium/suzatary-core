//! The title's laws, over the conversations held in core's memory.

use prodrome_core::event::Actor;
use suzatary_base::capability::Clock;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::{Conversation, Model, Turn};
use suzatary_conversations::secretary::store::Said;
use suzatary_conversations::secretary::store::{Entry, Kind, Namer, Stored};
use suzatary_lines::secretary::transcript;
use suzatary_topics::secretary::title::*;

fn stored(kind: Kind, title: &str, turns: Vec<Turn>) -> Stored {
    let mut entries = vec![Entry::Opened {
        at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
        title: suzatary_memory::secretary::words::Words::kept(title.to_owned()),
        model: "m".to_owned(),
        kind,
    }];
    entries.extend(turns.into_iter().map(|turn| Entry::Said {
        at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 1, 0, 0).expect("an instant"),
        turn,
        by: None,
    }));
    Stored::of(
        &ConversationId::new("2026-09-25-1").expect("a slug"),
        &entries,
    )
    .expect("a conversation")
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

/// The question carries the cue, the first question and the first answer.
#[test]
fn the_question_is_the_first_exchange() {
    let asked = question(&stored(
        Kind::Topic,
        "",
        vec![
            person("When is the permit due?"),
            Turn::Tool {
                verb: "search".into(),
                answer: "secret mail".to_owned(),
                outcome: suzatary_conversations::secretary::outcome::Outcome::Answered,
                files: Vec::new(),
            },
            answer("On the 3rd."),
            person("And the fee?"),
        ],
    ))
    .expect("a question");
    let [Turn::Person { text, .. }] = asked.turns.as_slice() else {
        panic!("one person turn");
    };
    assert!(text.starts_with(ASK));
    assert!(text.contains("When is the permit due?") && text.contains("On the 3rd."));
    assert!(!text.contains("secret mail") && !text.contains("fee"));
}

/// The one conversation the job runs over, and the clock it stamps with.
struct Lines(suzatary_secretary::stores::conversations::InMemory);

impl Clock for Lines {
    fn now(&self) -> suzatary_base::capability::Datetime {
        suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 5, 0, 0).expect("an instant")
    }
}

struct Says(&'static str);

impl Model for Says {
    fn name(&self) -> String {
        "titler".to_owned()
    }
    fn turn(
        &self,
        _: &Conversation,
        _: &serde_json::Value,
    ) -> Result<
        suzatary_conversations::secretary::model::Reply,
        suzatary_conversations::secretary::model::ModelError,
    > {
        Ok(suzatary_conversations::secretary::model::Reply::parse(
            self.0,
        ))
    }
}

fn lines_of(stored: &Stored) -> Lines {
    let mut entries = vec![Entry::Opened {
        at: stored.opened,
        title: stored.title.clone(),
        model: stored.model.clone(),
        kind: stored.kind,
    }];
    entries.extend(
        stored
            .turns
            .iter()
            .map(|Said { at, turn, by }| Entry::Said {
                at: *at,
                turn: turn.clone(),
                by: by.clone(),
            }),
    );
    let lines = suzatary_secretary::stores::conversations::InMemory::default();
    let id = ConversationId::new("2026-09-25-1").expect("a slug");
    transcript::start(lines.conversations(), &id, &entries[0]).expect("opens");
    for entry in &entries[1..] {
        transcript::push(lines.conversations(), &id, entry).expect("pushes");
    }
    Lines(lines)
}

/// The job names an owed topic once, and a rename wins after it.
#[test]
fn the_job_names_once_and_a_rename_is_an_append() {
    let id = ConversationId::new("2026-09-25-1").expect("a slug");
    let caps = lines_of(&stored(
        Kind::Topic,
        "",
        vec![person("When is the permit due?"), answer("On the 3rd.")],
    ));
    let named = run(caps.0.conversations(), &caps, &Says("Permit deadline"), &id)
        .expect("the job ran")
        .expect("a title landed");
    assert_eq!(
        named.title.get().map(String::as_str),
        Some("Permit deadline")
    );
    assert_eq!(
        run(caps.0.conversations(), &caps, &Says("Other"), &id).expect("ran"),
        None
    );
    let ada = Actor::new("ada").expect("an actor");
    let renamed =
        rename(caps.0.conversations(), &caps, &ada, &id, "  The permit  ").expect("renamed");
    assert_eq!(renamed.title.get().map(String::as_str), Some("The permit"));
    assert!(matches!(
        transcript::lines(caps.0.conversations(), &id).expect("reads").last().cloned(),
        Some(Entry::Titled { by: Namer::Person { actor }, .. }) if actor == "ada"
    ));
    assert_eq!(
        rename(caps.0.conversations(), &caps, &ada, &id, "two\nlines")
            .expect_err("refused")
            .status(),
        suzatary_base::refusal::Status::BadRequest
    );
}

/// An answer that is no title is a refusal and writes nothing.
#[test]
fn a_non_title_writes_nothing() {
    let id = ConversationId::new("2026-09-25-1").expect("a slug");
    let caps = lines_of(&stored(
        Kind::Topic,
        "",
        vec![person("When?"), answer("Soon.")],
    ));
    let before = transcript::lines(caps.0.conversations(), &id)
        .expect("reads")
        .len();
    assert!(run(caps.0.conversations(), &caps, &Says("#panel[]"), &id).is_err());
    assert_eq!(
        transcript::lines(caps.0.conversations(), &id)
            .expect("reads")
            .len(),
        before
    );
}
