//! Push when nobody is watching, over a real server with `v4` committed to `ada@example.com`
//! (see `common`'s header on peer identities): a round that answers with no subscriber to
//! `GET …/stream` pushes exactly once, and with a subscriber connected it pushes nothing.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

/// Like `tests/stream.rs::writer`, and also hands back the [`Fakes`], a second handle on the
/// same `Arc`, to read what [`common::FakeNotify`] recorded.
fn writer(tag: &str, model: FakeModel) -> (String, Arc<Fakes>, Arc<suzatary_cli::serve::Workers>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(Fakes::new(tag).whois(whois).model(model));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, workers, .. } =
        common::Server::new(Arc::clone(&fakes), deployment, Vec::new()).start();
    (base, fakes, workers)
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

fn say(base: &str, id: &str, text: &str) -> ureq::http::Response<ureq::Body> {
    common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": text}),
    )
}

/// Poll for the push, not just for idle: the worker writes the final turn and only then pushes,
/// so `idle` says nothing about the push having been sent. Bounded, so a round that never
/// pushes fails rather than hangs.
fn pushed(fakes: &Fakes) -> Vec<suzatary_core::capability::Push> {
    for _ in 0..400 {
        let pushed = fakes.notify.pushed();
        if !pushed.is_empty() {
            return pushed;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Vec::new()
}

/// A round that answers, with no subscriber, pushes exactly once: the conversation's
/// [`suzatary_core::capability::Push::Answered`], none of the answer's words.
#[test]
fn no_subscriber_earns_one_push_naming_the_conversation() {
    let (base, fakes, _workers) = writer(
        "push-no-subscriber",
        FakeModel::saying(["{\"say\": \"All good. Nothing urgent.\"}"]),
    );
    let id = open(&base);

    assert_eq!(common::status(&say(&base, &id, "What changed?")), 202);
    common::settled(&base, &id);

    let pushed = pushed(&fakes);
    assert_eq!(
        pushed.len(),
        1,
        "exactly one push for the one round: {pushed:?}"
    );
    assert_eq!(
        pushed[0],
        suzatary_core::capability::Push::Answered {
            conversation: suzatary_core::conversation_id::ConversationId::new(id.as_str())
                .expect("a slug"),
            brief: false,
        }
    );
    assert_eq!(
        pushed[0].printed("https://example.test").click,
        format!("https://example.test/#/secretary/{id}")
    );
}

/// The same round, with a subscriber connected throughout, pushes nothing:
/// the answer is already landing on the page a subscriber is reading.
#[test]
fn a_connected_subscriber_gets_no_push_at_all() {
    let (base, fakes, _workers) = writer(
        "push-with-subscriber",
        FakeModel::saying(["{\"say\": \"All good. Nothing urgent.\"}"]),
    );
    let id = open(&base);

    // Subscribe synchronously before the round runs: a blocking `.call()` returns only once the
    // subscription exists.
    let response = common::agent()
        .get(format!("{base}/api/secretary/conversations/{id}/stream"))
        .call()
        .expect("the stream connects");
    // Held for the whole scene: dropping `response` would close the subscription.
    let _held = response;

    assert_eq!(common::status(&say(&base, &id, "What changed?")), 202);
    common::settled(&base, &id);

    assert_eq!(
        fakes.notify.pushed(),
        Vec::new(),
        "a subscriber was watching; nothing should have been pushed"
    );
}
