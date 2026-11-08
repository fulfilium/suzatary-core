//! An act's note lands on the event and, when non-empty, appends one `Turn::Act` and wakes the
//! secretary, through both `/api/author` and a sealed `POST /api/objects`.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}
/// A writing server over the fixture's chain with a scripted model, and its `Fakes`.
fn writing(tag: &str, replies: impl IntoIterator<Item = &'static str>) -> (String, Arc<Fakes>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(whois)
            .model(FakeModel::saying(replies)),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    (base, fakes)
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment([ada()], [(ada(), common::actor("ada"))])
}

/// The conversation once the round the act started has answered.
fn heard_the_model(base: &str) -> serde_json::Value {
    common::poll_conversation(base, "default", |payload| {
        payload["turns"]
            .as_array()
            .and_then(|turns| turns.last())
            .is_some_and(|turn| turn["who"] == "model")
    })
}

/// A note on an update lands on the event and wakes the secretary.
#[test]
fn a_note_on_update_lands_on_the_event_and_wakes_the_secretary() {
    let (base, _fakes) = writing(
        "acts-update-note",
        [r#"{"say": "Understood, I'll leave it as is otherwise."}"#],
    );

    let mut authored = common::act(
        &format!("{base}/api/author"),
        &serde_json::json!({
            "update": {
                "todo": "file-permit",
                "body": "File the *permit* renewal, revised",
                "note": "the meeting moved to October",
            }
        }),
    );
    assert_eq!(
        common::status(&authored),
        200,
        "{:?}",
        common::body_json(&mut authored)
    );
    let print = common::body_json(&mut authored)["event"]
        .as_str()
        .expect("an event print")
        .to_owned();
    let event: suzatary_core::record::TodoEvent = prodrome_core::event::parse_event(
        &suzatary_core::record::TodoVocabulary::default(),
        &print,
    )
    .expect("the core printed it");
    match event {
        suzatary_core::record::TodoEvent::Authored(record) => {
            assert_eq!(record.payload.note, "the meeting moved to October");
        }
        other => panic!("update builds an Authored event, got {other:?}"),
    }

    let heard = heard_the_model(&base);
    let turns = heard["turns"].as_array().expect("turns");
    let act_at = turns
        .iter()
        .position(|turn| turn["who"] == "act")
        .expect("an act turn");
    let act = &turns[act_at];
    assert_eq!(
        act["text"], "Ada updated it: the meeting moved to October",
        "{turns:?}"
    );
    assert_eq!(
        act["references"],
        serde_json::json!(["todo:life/file-permit"]),
        "the act points at the todo it changed"
    );
    let last = turns.last().expect("a last turn");
    assert_eq!(last["who"], "model", "the note started a round: {turns:?}");
    assert_eq!(last["text"], "Understood, I'll leave it as is otherwise.");
}

/// An update with no note leaves `note` empty and the conversation untouched.
#[test]
fn an_update_with_no_note_appends_nothing_to_the_conversation() {
    let (base, _fakes) = writing("acts-update-blank", Vec::new());

    let mut authored = common::act(
        &format!("{base}/api/author"),
        &serde_json::json!({
            "update": {"todo": "file-permit", "body": "File the *permit* renewal, revised"}
        }),
    );
    assert_eq!(common::status(&authored), 200);
    let print = common::body_json(&mut authored)["event"]
        .as_str()
        .expect("an event print")
        .to_owned();
    let event: suzatary_core::record::TodoEvent = prodrome_core::event::parse_event(
        &suzatary_core::record::TodoVocabulary::default(),
        &print,
    )
    .expect("the core printed it");
    match event {
        suzatary_core::record::TodoEvent::Authored(record) => {
            assert_eq!(record.payload.note, "", "empty note is today's behaviour");
        }
        other => panic!("update builds an Authored event, got {other:?}"),
    }

    // Reading the conversations never opens one.
    std::thread::sleep(std::time::Duration::from_millis(200));
    let payload = common::listing(&base);
    assert_eq!(
        payload["conversations"].as_array().expect("a list").len(),
        0,
        "no note, so nothing opened the default conversation: {payload:?}"
    );
}

/// A sealed `Completed` with a note is heard when the box adopts it.
#[test]
fn a_sealed_completed_event_with_a_note_wakes_the_secretary_on_adoption() {
    let (base, fakes) = writing(
        "acts-sealed-note",
        [r#"{"say": "Noted — good to know it moved."}"#],
    );

    let event = prodrome_core::event::mk_completed::<suzatary_core::record::Record>(
        "file-permit",
        common::now(),
        "ada",
        "the meeting moved to October",
    )
    .expect("a completed event");
    let object = common::written(&fakes.chain, event);
    let mut posted = common::act(
        &format!("{base}/api/objects"),
        &serde_json::json!({"objects": common::signed_prints(&object)}),
    );
    assert_eq!(
        common::status(&posted),
        200,
        "{:?}",
        common::body_json(&mut posted)
    );

    let heard = heard_the_model(&base);
    let turns = heard["turns"].as_array().expect("turns");
    let act = turns
        .iter()
        .find(|turn| turn["who"] == "act")
        .expect("an act turn");
    assert_eq!(
        act["text"],
        "Ada completed it: the meeting moved to October"
    );
    assert_eq!(
        act["references"],
        serde_json::json!(["todo:life/file-permit"])
    );
    let last = turns.last().expect("a last turn");
    assert_eq!(last["who"], "model", "the note started a round: {turns:?}");
    assert_eq!(last["text"], "Noted — good to know it moved.");
}

/// A sealed `Completed` with no note appends nothing.
#[test]
fn a_sealed_completed_event_with_no_note_appends_nothing() {
    let (base, fakes) = writing("acts-sealed-blank", Vec::new());

    let event = prodrome_core::event::mk_completed::<suzatary_core::record::Record>(
        "file-permit",
        common::now(),
        "ada",
        "",
    )
    .expect("a completed event");
    let object = common::written(&fakes.chain, event);
    let posted = common::act(
        &format!("{base}/api/objects"),
        &serde_json::json!({"objects": common::signed_prints(&object)}),
    );
    assert_eq!(common::status(&posted), 200);

    std::thread::sleep(std::time::Duration::from_millis(200));
    let payload = common::listing(&base);
    assert_eq!(
        payload["conversations"].as_array().expect("a list").len(),
        0,
        "no note, so adoption opened no conversation: {payload:?}"
    );
}

/// The conversation once the model's last answer is `last`.
fn answered(base: &str, last: &str) -> serde_json::Value {
    common::poll_conversation(base, "default", |payload| {
        payload["turns"]
            .as_array()
            .and_then(|turns| turns.last())
            .is_some_and(|turn| turn["who"] == "model" && turn["text"] == last)
    })
}

/// What `remember` answered in the round an owner's note on `todo` started, the act line's stamp
/// as stored, and that line's own taint: the note goes through `/api/author`, and the model it
/// wakes calls `remember`.
fn remember_after_a_note_on(
    tag: &str,
    todo: &str,
    prepare: &dyn Fn(&Fakes),
) -> (
    serde_json::Value,
    Option<String>,
    suzatary_core::secretary::taint::Taint,
) {
    let (base, fakes) = writing(
        tag,
        [
            r#"{"say": "", "calls": [{"verb": "remember", "args": {
                "fact": "The owner keeps receipts", "kind": "owner",
                "why": "His note says so.", "sources": "mail:a1b2c3d4e5f6"}}]}"#,
            r#"{"say": "Noted."}"#,
        ],
    );
    prepare(&fakes);
    let authored = common::act(
        &format!("{base}/api/author"),
        &serde_json::json!({
            "update": {"todo": todo, "body": "Revised by the owner", "note": "paid at the counter"}
        }),
    );
    assert_eq!(common::status(&authored), 200);
    let heard = answered(&base, "Noted.");
    let turns = heard["turns"].as_array().expect("turns");
    let act = turns
        .iter()
        .find(|turn| turn["who"] == "act")
        .expect("an act turn");
    assert!(
        !act["text"].as_str().expect("text").contains(todo),
        "the sentence never names the todo: {act}"
    );
    let receipt = turns
        .iter()
        .find(|turn| turn["who"] == "verb" && turn["verb"] == "remember")
        .expect("remember answered");
    let answer: serde_json::Value =
        serde_json::from_str(receipt["text"].as_str().expect("text")).expect("json");
    let stored = suzatary_core::secretary::read(
        fakes.conversations.conversations(),
        &suzatary_core::conversation_id::DEFAULT,
    )
    .expect("reads");
    let line = stored
        .turns
        .iter()
        .find(|said| matches!(said.turn, suzatary_core::capability::Turn::Act { .. }))
        .expect("the act's line");
    (
        answer,
        line.by.clone().map(String::from),
        suzatary_core::secretary::taint::turn(line),
    )
}

/// A todo the agent wrote and nobody confirmed has an id mail may have chosen, which the model is
/// shown beside the owner's note: the box stamps the note's line unconfirmed where it writes it,
/// so the line is tainted and `remember` stores a card. The same note on a todo the owner wrote
/// is stamped his and its line is clean; that round is a card too, but only because the box loads
/// the todo it points at through `todo_show`, a read like any other.
#[test]
fn a_note_on_an_unconfirmed_agent_todo_makes_remember_a_card() {
    use suzatary_core::secretary::taint::Taint;
    let agents = common::CAST.todos.agents.id;
    let (answer, stamp, taint) = remember_after_a_note_on("acts-note-unconfirmed", agents, &|_| {});
    assert_eq!(
        stamp.as_deref(),
        Some("ada pointing at an unconfirmed todo")
    );
    assert_eq!(taint, Taint::Tainted);
    assert!(answer.get("proposed").is_some(), "a card: {answer}");

    let owners = common::CAST.todos.urgent.id;
    let (_, stamp, taint) = remember_after_a_note_on("acts-note-confirmed", owners, &|_| {});
    assert_eq!(stamp.as_deref(), Some("ada"));
    assert_eq!(taint, Taint::Clean);
}

/// The owner's turn that points at a todo is stamped by what it points at, where the say route
/// writes it: unconfirmed for the agent's todo, his own for one he wrote.
#[test]
fn a_turn_pointing_at_an_unconfirmed_agent_todo_is_stamped_so() {
    let (base, fakes) = writing(
        "acts-say-unconfirmed",
        [r#"{"say": "Seen."}"#, r#"{"say": "Seen."}"#],
    );
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "Pointing"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let stamped = |todo: &str| {
        let say = common::act(
            &format!("{base}/api/secretary/conversations/{id}/say"),
            &serde_json::json!({"say": "this one", "references": [format!("todo:life/{todo}")]}),
        );
        assert_eq!(common::status(&say), 202);
        let stored = suzatary_core::secretary::read(
            fakes.conversations.conversations(),
            &suzatary_core::conversation_id::ConversationId::new(id.clone()).expect("a slug"),
        )
        .expect("reads");
        stored
            .turns
            .iter()
            .rev()
            .find(|said| matches!(said.turn, suzatary_core::capability::Turn::Person { .. }))
            .and_then(|said| said.by.clone())
            .map(String::from)
    };
    assert_eq!(
        stamped(common::CAST.todos.agents.id).as_deref(),
        Some("ada pointing at an unconfirmed todo")
    );
    assert_eq!(
        stamped(common::CAST.todos.urgent.id).as_deref(),
        Some("ada")
    );
}

/// What decides is who wrote the todo's words: the agent marking the owner's own todo done is a
/// provisional claim, and leaves its content, id and all, his, so his note on it stays clean.
#[test]
fn a_note_on_his_own_todo_the_agent_claimed_done_stays_clean() {
    let owners = common::CAST.todos.urgent.id;
    let (_, stamp, taint) = remember_after_a_note_on("acts-note-claimed", owners, &|fakes| {
        suzatary_core::store::Prodrome::append(
            &fakes.chain,
            prodrome_core::event::mk_completed(owners, common::now(), "triage", "")
                .expect("an event"),
        )
        .expect("the agent's claim appends");
    });
    assert_eq!(stamp.as_deref(), Some("ada"));
    assert_eq!(taint, suzatary_core::secretary::taint::Taint::Clean);
}

/// An edited question is stamped by what it points at exactly as a said one is.
#[test]
fn an_edit_pointing_at_an_unconfirmed_agent_todo_is_stamped_so() {
    let (base, fakes) = writing(
        "acts-edit-unconfirmed",
        [r#"{"say": "Seen."}"#, r#"{"say": "Seen again."}"#],
    );
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "Editing"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let say = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "this one"}),
    );
    assert_eq!(common::status(&say), 202);
    common::poll_conversation(&base, &id, |payload| {
        payload["state"] == "idle"
            && payload["turns"]
                .as_array()
                .and_then(|turns| turns.last())
                .is_some_and(|turn| turn["who"] == "model")
    });
    let edited = common::act(
        &format!("{base}/api/secretary/conversations/{id}/edit"),
        &serde_json::json!({
            "say": "this one, rather",
            "references": [format!("todo:life/{}", common::CAST.todos.agents.id)]
        }),
    );
    assert_eq!(common::status(&edited), 202);
    let stored = suzatary_core::secretary::read(
        fakes.conversations.conversations(),
        &suzatary_core::conversation_id::ConversationId::new(id).expect("a slug"),
    )
    .expect("reads");
    let line = stored
        .turns
        .iter()
        .rev()
        .find(|said| matches!(said.turn, suzatary_core::capability::Turn::Person { .. }))
        .expect("the edited question");
    assert_eq!(
        line.by.clone().map(String::from).as_deref(),
        Some("ada pointing at an unconfirmed todo")
    );
    assert_eq!(
        suzatary_core::secretary::taint::turn(line),
        suzatary_core::secretary::taint::Taint::Tainted
    );
}
