//! Asking again over a real server, as a tab asks it: an `Again` line handed to the lines sets the
//! last round aside and the worker answers in a fresh session; an edit is that line and the new
//! question resting on it; a round that wrote anything is refused.

use crate::common;

use std::sync::Arc;

use common::{FakeModel, Fakes};
use suzatary_conversations::secretary::store::Rewind;

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
    common::opened(base, "A morning")
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
fn a_regenerate_and_an_edit_ask_again_over_the_tab_s_lines() {
    let (base, fakes) = serving(
        "rewind-again",
        FakeModel::saying([
            r#"{"say": "one"}"#,
            r#"{"say": "two"}"#,
            r#"{"say": "three"}"#,
        ]),
    );
    let id = open(&base);
    let question = common::say(&base, &id, "what changed");
    assert_eq!(
        common::settled(&base, &id)["again"],
        0,
        "the box offers the question it would ask again"
    );

    let _ = common::asked_again(&base, &id, &question, Rewind::Regenerate);
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
            suzatary_conversations::secretary::model::Turn::Model { text, .. } if text == "one"
        )),
        "the set-aside answer is never delivered"
    );

    let _ = common::edited(&base, &id, &question, "what changed today");
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
    let question = common::say(&base, &id, "I moved to Oslo, remember that");
    common::settled(&base, &id);
    for how in [Rewind::Regenerate, Rewind::Edit] {
        let sealed = common::sealed(&base, &id, &common::again_line(&base, &question, how));
        let refused = common::refused_line(&base, &sealed);
        assert!(refused.contains("never hides one"), "{refused}");
    }
    let after = common::settled(&base, &id);
    assert!(
        shape(&after).iter().all(|(_, _, aside)| !aside),
        "nothing was set aside"
    );
    assert!(after["again"].is_null(), "and nothing is offered: {after}");
}
