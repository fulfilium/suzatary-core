//! Stop over a real server: `POST /api/secretary/conversations/<id>/stop` pulls the running
//! round's halt; the round ends its model process and writes the act, which closes the question
//! in that one write, and the stop answers once it is written.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes, Gate};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn open(base: &str) -> String {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    assert_eq!(common::status(&opened), 200);
    common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

fn stop(base: &str, id: &str) -> ureq::http::Response<ureq::Body> {
    common::act(
        &format!("{base}/api/secretary/conversations/{id}/stop"),
        &serde_json::json!({}),
    )
}

/// Nothing running is nothing to stop: a 409, and no halt left standing to
/// stop the next question instead.
#[test]
fn a_stop_with_no_round_running_is_refused_and_stops_nothing_later() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new("stop-idle")
        .whois(whois)
        .model(FakeModel::saying(["{\"say\": \"here\"}"]));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::new(fakes), deployment, Vec::new())
        .start()
        .base;
    let id = open(&base);
    assert_eq!(common::status(&stop(&base, &id)), 409);

    let payload = common::said(&base, &id, "still there?");
    let last = payload["turns"]
        .as_array()
        .and_then(|turns| turns.last())
        .cloned();
    assert_eq!(
        last.as_ref().map(|turn| turn["text"].clone()),
        Some(serde_json::json!("here")),
        "the next question is answered: {payload}"
    );
}

/// Each turn as `(who, text, queued)`.
fn shape(payload: &serde_json::Value) -> Vec<(String, String, bool)> {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .map(|turn| {
            (
                turn["who"].as_str().unwrap_or_default().to_owned(),
                turn["text"].as_str().unwrap_or_default().to_owned(),
                turn["queued"].as_bool().unwrap_or_default(),
            )
        })
        .collect()
}

/// A stop mid-round ends it: the answer never lands, the act names who stopped it and closes the
/// question by itself, with no empty answer after it. From the stop's own answer on, there is no
/// reading in which the stopped question is queued: the 202 is given once the act is written.
#[test]
fn a_stop_mid_round_ends_it_in_the_act() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new("stop-mid")
            .whois(whois)
            .model(FakeModel::saying(["{\"say\": \"never shown\"}"])),
    );
    let gate = Gate::new();
    fakes.model.hold_next(gate.clone());
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    let id = open(&base);
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "read every thread"}),
    );
    assert_eq!(common::status(&said), 202);
    gate.wait_until_entered();

    // Held in the model: the round has written nothing, and the question is owed to it.
    let held = common::reading(&base, &id, &[]);
    assert_eq!(
        shape(&held),
        [("person".to_owned(), "read every thread".to_owned(), true)],
        "{held}"
    );

    let stopped_by = suzatary_core::secretary::store::stopped_by(common::CAST.owner.actor);
    let expected = vec![
        ("person".to_owned(), "read every thread".to_owned(), false),
        ("act".to_owned(), stopped_by, false),
    ];
    let mut stopped = stop(&base, &id);
    assert_eq!(common::status(&stopped), 202);
    // The stop's own answer is the first reading after it: the act is written and closes the
    // question, before the worker has done anything after its round.
    let stopped = common::body_json(&mut stopped);
    assert_eq!(shape(&stopped), expected, "the stop's answer: {stopped}");
    assert_eq!(stopped["state"], "idle", "{stopped}");

    let payload = common::settled(&base, &id);
    assert_eq!(shape(&payload), expected, "read again: {payload}");
    assert_eq!(
        common::status(&stop(&base, &id)),
        409,
        "a stopped round is over"
    );
}

/// A question the page shows as queued, which no round has taken yet, is stopped by the act
/// alone: the stop answers with the question closed, no model is asked for it, and the next
/// question is answered as usual, so nothing of the stop is left over.
#[test]
fn a_stop_before_any_round_takes_the_question_closes_it() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new("stop-queued")
            .whois(whois)
            .model(FakeModel::saying(["{\"say\": \"here\"}"])),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    let id = open(&base);
    // Written with no nudge, as a `say` stands between its write and the worker's round.
    let conversation =
        suzatary_core::conversation_id::ConversationId::new(&id).expect("a conversation id");
    suzatary_core::secretary::transcript::push(
        fakes.conversations.conversations(),
        &conversation,
        &suzatary_core::secretary::store::Entry::Said {
            at: suzatary_core::capability::Datetime::new(2026, 9, 2, 12, 0, 0, 0)
                .expect("an instant"),
            turn: suzatary_core::capability::Turn::Person {
                text: "read every thread".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
    )
    .expect("the question appends");

    let mut stopped = stop(&base, &id);
    assert_eq!(common::status(&stopped), 202);
    let stopped = common::body_json(&mut stopped);
    let stopped_by = suzatary_core::secretary::store::stopped_by(common::CAST.owner.actor);
    assert_eq!(
        shape(&stopped),
        [
            ("person".to_owned(), "read every thread".to_owned(), false),
            ("act".to_owned(), stopped_by, false),
        ],
        "the stop's answer: {stopped}"
    );
    assert_eq!(stopped["state"], "idle", "{stopped}");
    assert!(fakes.model.asked().is_empty(), "no model was asked");
    assert_eq!(
        common::status(&stop(&base, &id)),
        409,
        "what was stopped is not stopped twice"
    );

    let payload = common::said(&base, &id, "still there?");
    assert_eq!(
        payload["turns"]
            .as_array()
            .and_then(|turns| turns.last())
            .map(|turn| turn["text"].clone()),
        Some(serde_json::json!("here")),
        "the next question is answered: {payload}"
    );
}
