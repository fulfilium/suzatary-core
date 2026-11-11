//! Asking again over a real server: `POST …/regenerate` and `POST …/edit` set the last round
//! aside and the worker answers in a fresh session; a round that wrote anything is refused.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_base::capability::Login {
    common::login("ada@example.com")
}

fn serving(tag: &str, model: FakeModel) -> (String, Arc<Fakes>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(Fakes::new(tag).whois(whois).model(model));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    (base, fakes)
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

fn post(base: &str, id: &str, verb: &str, body: &serde_json::Value) -> u16 {
    common::status(&common::act(
        &format!("{base}/api/secretary/conversations/{id}/{verb}"),
        body,
    ))
}

/// Each turn as (who, text, set aside).
fn shape(payload: &serde_json::Value) -> Vec<(String, String, bool)> {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .map(|turn| {
            (
                turn["who"].as_str().unwrap_or_default().to_owned(),
                turn["text"].as_str().unwrap_or_default().to_owned(),
                turn["attempt"] == "earlier",
            )
        })
        .collect()
}

fn row(who: &str, text: &str, aside: bool) -> (String, String, bool) {
    (who.to_owned(), text.to_owned(), aside)
}

/// Regenerate, then edit: each sets the last round aside, keeps every turn
/// and index, and the next answer comes from a fresh session.
#[test]
fn a_regenerate_and_an_edit_ask_again_over_the_live_path() {
    let (base, fakes) = serving(
        "rewind-again",
        FakeModel::saying([
            r#"{"say": "one"}"#,
            r#"{"say": "two"}"#,
            r#"{"say": "three"}"#,
        ]),
    );
    let id = open(&base);
    assert_eq!(
        post(
            &base,
            &id,
            "say",
            &serde_json::json!({"say": "what changed"})
        ),
        202
    );
    assert_eq!(
        common::settled(&base, &id)["again"],
        0,
        "the box offers the question it would ask again"
    );

    assert_eq!(post(&base, &id, "regenerate", &serde_json::json!({})), 202);
    let regenerated = common::settled(&base, &id);
    assert_eq!(
        shape(&regenerated),
        vec![
            row("person", "what changed", false),
            row("model", "one", true),
            row("model", "two", false),
        ],
        "{regenerated}"
    );
    assert_eq!(
        fakes.model.opened().len(),
        2,
        "the session was dropped and reopened"
    );
    let told = fakes.model.asked().last().cloned().expect("asked");
    assert!(
        !told.turns.iter().any(|turn| matches!(
            turn,
            suzatary_core::secretary::model::Turn::Model { text, .. } if text == "one"
        )),
        "the set-aside answer is never delivered"
    );

    assert_eq!(
        post(
            &base,
            &id,
            "edit",
            &serde_json::json!({"say": "what changed today"})
        ),
        202
    );
    let edited = common::settled(&base, &id);
    assert_eq!(
        shape(&edited),
        vec![
            row("person", "what changed", true),
            row("model", "one", true),
            row("model", "two", true),
            row("person", "what changed today", false),
            row("model", "three", false),
        ],
        "{edited}"
    );
}

/// A round that wrote is not asked again: a rewind never hides a write, and
/// the refusal says to ask again instead.
#[test]
fn a_round_that_remembered_is_refused() {
    let (base, _) = serving(
        "rewind-wrote",
        FakeModel::saying([
            r#"{"say": "", "calls": [{"verb": "remember", "args": {
                "fact": "Ada moved to Oslo in September", "kind": "owner"}}]}"#,
            r#"{"say": "Noted."}"#,
        ]),
    );
    let id = open(&base);
    assert_eq!(
        post(
            &base,
            &id,
            "say",
            &serde_json::json!({"say": "I moved to Oslo, remember that"})
        ),
        202
    );
    common::settled(&base, &id);
    let mut refused = common::act(
        &format!("{base}/api/secretary/conversations/{id}/regenerate"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&refused), 409);
    let said = common::body_json(&mut refused);
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|error| error.contains("ask again")),
        "{said}"
    );
    assert_eq!(
        post(
            &base,
            &id,
            "edit",
            &serde_json::json!({"say": "forget that"})
        ),
        409
    );
    let after = common::settled(&base, &id);
    assert!(
        shape(&after).iter().all(|(_, _, aside)| !aside),
        "nothing was set aside"
    );
    assert!(after["again"].is_null(), "and nothing is offered: {after}");
}
