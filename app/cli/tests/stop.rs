//! Stop over a real server: a tab's `Stopped` line, handed to the lines, closes what it rests on
//! in that one line; its acceptance pulls the running round's halt, so the round ends its model
//! process and writes nothing more.

use crate::common;

use std::sync::Arc;

use common::{FakeModel, Fakes, Gate};
use suzatary_conversations::secretary::store;

fn ada() -> suzatary_base::capability::Login {
    common::login("ada@example.com")
}

fn open(base: &str) -> String {
    common::opened(base, "A morning")
}

/// Why the box refused a tab's stop of conversation `id`.
fn refused_stop(base: &str, id: &str) -> String {
    let stop = common::sealed(
        base,
        id,
        &store::Entry::Stopped {
            at: common::next_instant(),
            by: common::CAST.owner.actor.to_owned(),
        },
    );
    common::refused_line(base, &stop)
}

/// Nothing running is nothing to stop: the lines refuse the stop, and no halt is left standing
/// to stop the next question instead.
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
    assert!(refused_stop(&base, &id).contains("nothing stops"));

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
/// question by itself, with no empty answer after it. Once the box holds the stop, no reading
/// shows the stopped question queued or working.
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
    let _ = common::say(&base, &id, "read every thread");
    gate.wait_until_entered();

    // Held in the model: the round has claimed the question and written nothing more, so the
    // question is the round's and no longer queued.
    let held = common::reading(&base, &id, &[]);
    assert_eq!(held["state"], "thinking", "{held}");
    assert_eq!(
        shape(&held),
        [("person".to_owned(), "read every thread".to_owned(), false)],
        "{held}"
    );

    let stopped_by = store::stopped_by(common::CAST.owner.actor);
    let expected = vec![
        ("person".to_owned(), "read every thread".to_owned(), false),
        ("act".to_owned(), stopped_by, false),
    ];
    let _ = common::stop(&base, &id);
    // The first reading after the stop is taken: its line closes the question, whatever the
    // round has done since.
    let stopped = common::reading(&base, &id, &[]);
    assert_eq!(shape(&stopped), expected, "read after the stop: {stopped}");
    assert_eq!(stopped["state"], "idle", "{stopped}");

    let payload = common::settled(&base, &id);
    assert_eq!(shape(&payload), expected, "read again: {payload}");
    assert!(
        refused_stop(&base, &id).contains("nothing stops"),
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
        suzatary_base::conversation_id::ConversationId::new(&id).expect("a conversation id");
    suzatary_lines::secretary::transcript::push(
        fakes.conversations.conversations(),
        &conversation,
        &store::Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 2, 12, 0, 0, 0)
                .expect("an instant"),
            turn: suzatary_conversations::secretary::model::Turn::Person {
                text: "read every thread".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
    )
    .expect("the question appends");

    let _ = common::stop(&base, &id);
    let stopped = common::settled(&base, &id);
    let stopped_by = store::stopped_by(common::CAST.owner.actor);
    assert_eq!(
        shape(&stopped),
        [
            ("person".to_owned(), "read every thread".to_owned(), false),
            ("act".to_owned(), stopped_by, false),
        ],
        "read after the stop: {stopped}"
    );
    assert_eq!(stopped["state"], "idle", "{stopped}");
    assert!(fakes.model.asked().is_empty(), "no model was asked");
    assert!(
        refused_stop(&base, &id).contains("nothing stops"),
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
